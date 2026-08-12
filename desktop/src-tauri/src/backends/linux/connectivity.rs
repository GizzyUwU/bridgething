use std::{
  sync::{Arc, Mutex},
  thread,
};

use bridgething_companion::backend::{ConnectivityInbox, ConnectivityMonitor};
use futures::StreamExt;
use tokio::sync::oneshot;
use zbus::{Connection, Proxy};

const NM_SERVICE: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const NM_STATE_CONNECTED_LOCAL: u32 = 50;

#[derive(Default)]
pub struct NetworkManagerConnectivity {
  held: Mutex<Option<oneshot::Sender<()>>>,
}

impl ConnectivityMonitor for NetworkManagerConnectivity {
  fn start(&self, inbox: Arc<ConnectivityInbox>) {
    self.stop();

    let (stop, halted) = oneshot::channel();
    match thread::Builder::new()
      .name("bridgething-connectivity".to_owned())
      .spawn(move || watch(inbox, halted))
    {
      Ok(_) => *self.held.lock().unwrap() = Some(stop),
      Err(error) => tracing::warn!(%error, "the connectivity watcher could not be started"),
    }
  }

  fn stop(&self) {
    self.held.lock().unwrap().take();
  }
}

fn watch(inbox: Arc<ConnectivityInbox>, halted: oneshot::Receiver<()>) {
  match tokio::runtime::Builder::new_current_thread().enable_all().build() {
    Ok(runtime) => runtime.block_on(observe(inbox, halted)),
    Err(error) => {
      tracing::warn!(%error, "the connectivity watcher has no runtime; the desktop assumes it is online");
      inbox.on_changed(true);
    }
  }
}

async fn observe(inbox: Arc<ConnectivityInbox>, mut halted: oneshot::Receiver<()>) {
  let manager = match manager().await {
    Ok(manager) => manager,
    Err(error) => {
      tracing::warn!(%error, "networkmanager is not on the bus; the desktop assumes it is online");
      inbox.on_changed(true);
      return;
    }
  };

  let mut edges = match manager.receive_signal("StateChanged").await {
    Ok(edges) => edges,
    Err(error) => {
      tracing::warn!(%error, "networkmanager refused a state subscription; the desktop assumes it is online");
      inbox.on_changed(true);
      return;
    }
  };

  inbox.on_changed(reachable(
    manager.get_property("State").await.unwrap_or(NM_STATE_CONNECTED_LOCAL),
  ));

  loop {
    tokio::select! {
      _ = &mut halted => break,
      edge = edges.next() => {
        let Some(edge) = edge else { break };
        let body = edge.body();
        match body.deserialize::<u32>() {
          Ok(state) => inbox.on_changed(reachable(state)),
          Err(error) => tracing::warn!(%error, "networkmanager sent a state edge that is not a state"),
        }
      }
    }
  }
}

async fn manager() -> zbus::Result<Proxy<'static>> {
  let connection = Connection::system().await?;
  Proxy::new(&connection, NM_SERVICE, NM_PATH, NM_SERVICE).await
}

fn reachable(state: u32) -> bool {
  state >= NM_STATE_CONNECTED_LOCAL
}
