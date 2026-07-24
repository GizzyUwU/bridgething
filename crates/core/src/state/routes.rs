use std::{
  collections::HashMap,
  net::SocketAddr,
  sync::{Arc, RwLock},
};

use uuid::Uuid;

#[derive(Debug, Clone, Default)]
pub struct RouteTable {
  inner: Arc<RwLock<HashMap<Uuid, SocketAddr>>>,
}

impl RouteTable {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn register(&self, id: Uuid, owner: SocketAddr) {
    self.inner.write().expect("route table poisoned").insert(id, owner);
  }

  pub fn lookup(&self, id: Uuid) -> Option<SocketAddr> {
    self.inner.read().expect("route table poisoned").get(&id).copied()
  }

  pub fn drop_id(&self, id: Uuid) -> Option<SocketAddr> {
    self.inner.write().expect("route table poisoned").remove(&id)
  }

  pub fn drain_for_owner(&self, owner: SocketAddr) -> Vec<Uuid> {
    let mut guard = self.inner.write().expect("route table poisoned");
    let ids: Vec<Uuid> = guard
      .iter()
      .filter_map(|(id, addr)| (*addr == owner).then_some(*id))
      .collect();
    for id in &ids {
      guard.remove(id);
    }
    ids
  }

  pub fn drain_all(&self) -> Vec<(Uuid, SocketAddr)> {
    let mut guard = self.inner.write().expect("route table poisoned");
    guard.drain().collect()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn addr(s: &str) -> SocketAddr {
    s.parse().unwrap()
  }

  #[test]
  fn register_lookup_drop() {
    let table = RouteTable::new();
    let id = Uuid::now_v7();
    let a = addr("127.0.0.1:1000");
    table.register(id, a);
    assert_eq!(table.lookup(id), Some(a));
    assert_eq!(table.drop_id(id), Some(a));
    assert_eq!(table.lookup(id), None);
  }

  #[test]
  fn drain_for_owner_returns_only_matching() {
    let table = RouteTable::new();
    let alice = addr("127.0.0.1:1");
    let bob = addr("127.0.0.1:2");
    let a1 = Uuid::now_v7();
    let a2 = Uuid::now_v7();
    let b1 = Uuid::now_v7();
    table.register(a1, alice);
    table.register(a2, alice);
    table.register(b1, bob);

    let mut drained = table.drain_for_owner(alice);
    drained.sort();
    let mut expected = vec![a1, a2];
    expected.sort();
    assert_eq!(drained, expected);
    assert_eq!(table.lookup(a1), None);
    assert_eq!(table.lookup(b1), Some(bob));
  }

  #[test]
  fn drain_all_empties_table() {
    let table = RouteTable::new();
    let a = addr("127.0.0.1:1");
    let id1 = Uuid::now_v7();
    let id2 = Uuid::now_v7();
    table.register(id1, a);
    table.register(id2, a);
    let drained = table.drain_all();
    assert_eq!(drained.len(), 2);
    assert_eq!(table.lookup(id1), None);
    assert_eq!(table.lookup(id2), None);
  }
}
