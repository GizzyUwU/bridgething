use std::{
  collections::HashMap,
  sync::{Arc, RwLock},
};

use bytes::Bytes;
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use crate::bluetooth::Address;

#[derive(Debug)]
pub enum TunnelInbound {
  Data(Bytes),
  Closed(Option<String>),
}

#[derive(Debug)]
struct TunnelRoute {
  inbound: mpsc::Sender<TunnelInbound>,
  consumed: watch::Sender<u64>,
  gateway: Address,
}

#[derive(Debug, Clone, Default)]
pub struct TunnelRoutes {
  inner: Arc<RwLock<HashMap<Uuid, TunnelRoute>>>,
}

impl TunnelRoutes {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn register(&self, id: Uuid, inbound: mpsc::Sender<TunnelInbound>, gateway: Address) -> watch::Receiver<u64> {
    let (consumed, rx) = watch::channel(0);
    self.inner.write().expect("tunnel routes poisoned").insert(
      id,
      TunnelRoute {
        inbound,
        consumed,
        gateway,
      },
    );
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

  pub fn consumed(&self, id: Uuid) -> Option<u64> {
    self
      .inner
      .read()
      .expect("tunnel routes poisoned")
      .get(&id)
      .map(|route| *route.consumed.borrow())
  }

  pub fn drop_id(&self, id: Uuid) -> Option<mpsc::Sender<TunnelInbound>> {
    self
      .inner
      .write()
      .expect("tunnel routes poisoned")
      .remove(&id)
      .map(|route| route.inbound)
  }

  pub fn kill_for_gateway(&self, gateway: Address) -> usize {
    let mut routes = self.inner.write().expect("tunnel routes poisoned");
    let doomed: Vec<Uuid> = routes
      .iter()
      .filter(|(_, route)| route.gateway == gateway)
      .map(|(id, _)| *id)
      .collect();
    for id in &doomed {
      routes.remove(id);
    }
    doomed.len()
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
    let consumed = routes.register(id, tx, Address([0xAA, 0, 0, 0, 0, 1]));

    routes.note_ack(id, 4096);
    routes.note_ack(id, 4096);
    assert_eq!(*consumed.borrow(), 8192);
    assert_eq!(routes.consumed(id), Some(8192));

    routes.drop_id(id);
    routes.note_ack(id, 4096);
    assert_eq!(*consumed.borrow(), 8192, "a dropped route stops accumulating");
    assert_eq!(routes.consumed(id), None);
  }

  #[tokio::test]
  async fn killing_one_gateway_spares_another() {
    let routes = TunnelRoutes::new();
    let phone = Address([0xAA, 0, 0, 0, 0, 1]);
    let desktop = Address([0xAA, 0, 0, 0, 0, 2]);
    let phone_tunnel = Uuid::now_v7();
    let desktop_tunnel = Uuid::now_v7();
    let (tx_a, _rx_a) = mpsc::channel(1);
    let (tx_b, _rx_b) = mpsc::channel(1);
    routes.register(phone_tunnel, tx_a, phone);
    routes.register(desktop_tunnel, tx_b, desktop);

    assert_eq!(routes.kill_for_gateway(desktop), 1);
    assert!(
      routes.lookup(phone_tunnel).is_some(),
      "a peer companion killed this tunnel"
    );
    assert!(routes.lookup(desktop_tunnel).is_none());
  }

  #[tokio::test]
  async fn every_route_is_reaped_by_its_owner_leaving() {
    let routes = TunnelRoutes::new();
    let phone = Address([0xAA, 0, 0, 0, 0, 1]);
    let first = Uuid::now_v7();
    let second = Uuid::now_v7();
    let (tx_a, mut rx_a) = mpsc::channel(1);
    let (tx_b, mut rx_b) = mpsc::channel(1);
    routes.register(first, tx_a, phone);
    routes.register(second, tx_b, phone);

    assert_eq!(routes.kill_for_gateway(phone), 2);
    assert!(
      rx_a.recv().await.is_none() && rx_b.recv().await.is_none(),
      "a tunnel outlived the companion that owns it"
    );
  }
}
