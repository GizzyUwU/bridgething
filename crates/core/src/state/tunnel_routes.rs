use std::{
  collections::HashMap,
  sync::{Arc, RwLock},
};

use bytes::Bytes;
use tokio::sync::mpsc;
use uuid::Uuid;

#[derive(Debug)]
pub enum TunnelInbound {
  Data(Bytes),
  Closed(Option<String>),
}

#[derive(Debug, Clone, Default)]
pub struct TunnelRoutes {
  inner: Arc<RwLock<HashMap<Uuid, mpsc::Sender<TunnelInbound>>>>,
}

impl TunnelRoutes {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn register(&self, id: Uuid, sender: mpsc::Sender<TunnelInbound>) {
    self.inner.write().expect("tunnel routes poisoned").insert(id, sender);
  }

  pub fn lookup(&self, id: Uuid) -> Option<mpsc::Sender<TunnelInbound>> {
    self.inner.read().expect("tunnel routes poisoned").get(&id).cloned()
  }

  pub fn drop_id(&self, id: Uuid) -> Option<mpsc::Sender<TunnelInbound>> {
    self.inner.write().expect("tunnel routes poisoned").remove(&id)
  }

  pub fn kill_all(&self) {
    self.inner.write().expect("tunnel routes poisoned").clear();
  }
}
