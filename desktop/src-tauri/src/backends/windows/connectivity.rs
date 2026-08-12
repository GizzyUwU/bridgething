use std::sync::{Arc, Mutex};

use bridgething_companion::backend::{ConnectivityInbox, ConnectivityMonitor};
use windows::Networking::Connectivity::{
  NetworkConnectivityLevel, NetworkInformation, NetworkStatusChangedEventHandler,
};

#[derive(Default)]
pub struct NetworkInformationConnectivity {
  held: Mutex<Option<i64>>,
}

impl ConnectivityMonitor for NetworkInformationConnectivity {
  fn start(&self, inbox: Arc<ConnectivityInbox>) {
    self.stop();

    let held = Arc::clone(&inbox);
    let handler = NetworkStatusChangedEventHandler::new(move |_| {
      held.on_changed(reachable());
      Ok(())
    });
    match NetworkInformation::NetworkStatusChanged(&handler) {
      Ok(token) => {
        *self.held.lock().unwrap() = Some(token);
        inbox.on_changed(reachable());
      }
      Err(error) => tracing::warn!(%error, "windows refused a network watcher; connectivity edges are not observed"),
    }
  }

  fn stop(&self) {
    if let Some(token) = self.held.lock().unwrap().take() {
      let _ = NetworkInformation::RemoveNetworkStatusChanged(token);
    }
  }
}

fn reachable() -> bool {
  NetworkInformation::GetConnectionProfiles().is_ok_and(|profiles| {
    profiles.into_iter().any(|profile| {
      profile
        .GetNetworkConnectivityLevel()
        .is_ok_and(|level| level != NetworkConnectivityLevel::None)
    })
  })
}
