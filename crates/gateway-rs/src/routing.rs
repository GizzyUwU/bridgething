use std::{collections::HashMap, mem::Discriminant, sync::Arc};

use bridgething_sdk_runtime::{Connection, rt};
use libbridgething::gateway::{BridgeToGatewayMsg, BridgeToGatewayMsgData};
use tokio::sync::{broadcast, mpsc};

use crate::{Gateway, GatewayHandlers, GatewayProtocol, route};

const SURFACE_QUEUE: usize = 16;

pub struct Routing {
  conn: Connection<GatewayProtocol>,
  #[cfg(not(target_arch = "wasm32"))]
  handle: tokio::task::JoinHandle<()>,
}

impl Routing {
  pub async fn closed(&self) {
    self.conn.closed().await;
  }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for Routing {
  fn drop(&mut self) {
    self.handle.abort();
  }
}

pub fn spawn_routing<H>(gateway: Gateway, handlers: Arc<H>, inbound: broadcast::Receiver<BridgeToGatewayMsg>) -> Routing
where
  H: GatewayHandlers + 'static,
{
  let conn = gateway.connection().clone();
  let driving = drive(gateway, handlers, inbound);

  #[cfg(not(target_arch = "wasm32"))]
  return Routing {
    conn,
    handle: tokio::spawn(driving),
  };

  #[cfg(target_arch = "wasm32")]
  {
    bridgething_sdk_runtime::rt::spawn(driving);
    Routing { conn }
  }
}

async fn drive<H>(gateway: Gateway, handlers: Arc<H>, mut inbound: broadcast::Receiver<BridgeToGatewayMsg>)
where
  H: GatewayHandlers + 'static,
{
  let closed = gateway.connection().closed();
  tokio::pin!(closed);
  let mut surfaces: HashMap<Discriminant<BridgeToGatewayMsgData>, mpsc::Sender<BridgeToGatewayMsg>> = HashMap::new();

  loop {
    let received = tokio::select! {
      biased;
      () = &mut closed => break,
      received = inbound.recv() => received,
    };
    let msg = match received {
      Ok(msg) => msg,
      Err(broadcast::error::RecvError::Lagged(dropped)) => {
        tracing::warn!(dropped, "the routing path fell behind the link");
        continue;
      }
      Err(broadcast::error::RecvError::Closed) => break,
    };

    let surface = std::mem::discriminant(&msg.data);
    let server = surfaces.entry(surface).or_insert_with(|| {
      let (tx, rx) = mpsc::channel(SURFACE_QUEUE);
      rt::spawn(serve(gateway.clone(), handlers.clone(), rx));
      tx
    });
    if server.send(msg).await.is_err() {
      tracing::warn!("a surface server stopped; the next message on it starts a new one");
      surfaces.remove(&surface);
    }
  }
}

async fn serve<H>(gateway: Gateway, handlers: Arc<H>, mut queue: mpsc::Receiver<BridgeToGatewayMsg>)
where
  H: GatewayHandlers + 'static,
{
  while let Some(msg) = queue.recv().await {
    if let Err(error) = route(&handlers, msg, gateway.connection()).await {
      tracing::warn!(?error, "an inbound message could not be answered");
    }
  }
}
