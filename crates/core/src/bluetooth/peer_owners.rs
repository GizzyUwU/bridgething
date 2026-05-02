//! Registry of which transport currently holds the connection to each
//! gateway peer. Each transport (rfcomm, iap2 ea, network) registers on
//! peer connect and unregisters on disconnect; `GatewayMan::send_all`
//! consults the map to route addressed sends to a single transport
//! (no fan-out clones) and broadcast sends only to transports that
//! currently have ≥1 peer.
//!
//! Idempotent: `register(addr, kind)` is safe to call repeatedly with
//! the same address (e.g. a transport that opens multiple session-level
//! channels per peer); the first matching `unregister` removes the
//! entry, so callers must avoid mismatched register/unregister counts.

use std::{
  collections::{HashMap, HashSet},
  sync::{Arc, Mutex},
};

use bluer::Address;

use super::GatewayType;

#[derive(Debug, Clone, Default)]
pub struct PeerOwners {
  inner: Arc<Mutex<HashMap<Address, GatewayType>>>,
}

impl PeerOwners {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn register(&self, addr: Address, kind: GatewayType) {
    let mut map = self.inner.lock().expect("peer_owners poisoned");
    if let Some(existing) = map.get(&addr).copied() {
      if existing != kind {
        tracing::warn!(
          %addr,
          ?existing,
          incoming = ?kind,
          "peer_owners: address already owned by a different transport - keeping prior owner"
        );
      }
      return;
    }
    map.insert(addr, kind);
  }

  pub fn unregister(&self, addr: Address, kind: GatewayType) {
    let mut map = self.inner.lock().expect("peer_owners poisoned");
    if let Some(existing) = map.get(&addr).copied()
      && existing == kind
    {
      map.remove(&addr);
    }
  }

  pub fn owner(&self, addr: &Address) -> Option<GatewayType> {
    self.inner.lock().expect("peer_owners poisoned").get(addr).copied()
  }

  pub fn active_kinds(&self) -> HashSet<GatewayType> {
    self
      .inner
      .lock()
      .expect("peer_owners poisoned")
      .values()
      .copied()
      .collect()
  }
}
