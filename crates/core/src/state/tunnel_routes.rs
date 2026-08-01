use std::{
  collections::HashMap,
  sync::{Arc, RwLock},
};

use bytes::Bytes;
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

#[derive(Debug)]
pub enum TunnelInbound {
  Data(Bytes),
  Closed(Option<String>),
}

#[derive(Debug)]
struct TunnelRoute {
  inbound: mpsc::Sender<TunnelInbound>,
  consumed: watch::Sender<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct TunnelRoutes {
  inner: Arc<RwLock<HashMap<Uuid, TunnelRoute>>>,
}

impl TunnelRoutes {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn register(&self, id: Uuid, inbound: mpsc::Sender<TunnelInbound>) -> watch::Receiver<u64> {
    let (consumed, rx) = watch::channel(0);
    self
      .inner
      .write()
      .expect("tunnel routes poisoned")
      .insert(id, TunnelRoute { inbound, consumed });
    rx
  }

  pub fn lookup(&self, id: Uuid) -> Option<mpsc::Sender<TunnelInbound>> {
    self
      .inner
      .read()
      .expect("tunnel routes poisoned")
      .get(&id)
      .map(|route| route.inbound.clone())
  }

  pub fn note_ack(&self, id: Uuid, consumed: u32) {
    if let Some(route) = self.inner.read().expect("tunnel routes poisoned").get(&id) {
      route.consumed.send_modify(|total| *total += u64::from(consumed));
    }
  }

  pub fn drop_id(&self, id: Uuid) -> Option<mpsc::Sender<TunnelInbound>> {
    self
      .inner
      .write()
      .expect("tunnel routes poisoned")
      .remove(&id)
      .map(|route| route.inbound)
  }

  pub fn kill_all(&self) -> usize {
    let mut routes = self.inner.write().expect("tunnel routes poisoned");
    let killed = routes.len();
    routes.clear();
    killed
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn acks_accumulate_as_deltas() {
    let routes = TunnelRoutes::new();
    let id = Uuid::now_v7();
    let (tx, _rx) = mpsc::channel(1);
    let consumed = routes.register(id, tx);

    routes.note_ack(id, 4096);
    routes.note_ack(id, 4096);
    assert_eq!(*consumed.borrow(), 8192);

    routes.drop_id(id);
    routes.note_ack(id, 4096);
    assert_eq!(*consumed.borrow(), 8192, "a dropped route stops accumulating");
  }
}
