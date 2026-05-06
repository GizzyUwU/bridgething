//! Per-webapp geo watch tracker. Each webapp's `geo.watch` registers a
//! token with desired (accuracy, min_interval_ms); the daemon aggregates
//! across watchers (most-demanding accuracy, smallest interval) and
//! forwards a single `Watch` to the companion. When the aggregate
//! changes (first watcher / last watcher / accuracy upgrade / interval
//! drop), the daemon reissues the gateway-side request.

use std::{
  collections::HashMap,
  net::SocketAddr,
  sync::{Arc, RwLock},
};

use libbridgething::GeoAccuracy;
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
struct Watcher {
  owner: SocketAddr,
  accuracy: GeoAccuracy,
  min_interval_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchAggregate {
  pub accuracy: GeoAccuracy,
  pub min_interval_ms: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct WatchChange {
  pub existed: bool,
  pub prev: Option<WatchAggregate>,
  pub next: Option<WatchAggregate>,
}

#[derive(Debug, Clone, Default)]
pub struct GeoWatchers {
  inner: Arc<RwLock<HashMap<Uuid, Watcher>>>,
}

impl GeoWatchers {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn register(&self, token: Uuid, owner: SocketAddr, accuracy: GeoAccuracy, min_interval_ms: u32) -> WatchChange {
    let mut guard = self.inner.write().expect("geo watchers poisoned");
    let prev = aggregate_of(&guard);
    guard.insert(
      token,
      Watcher {
        owner,
        accuracy,
        min_interval_ms,
      },
    );
    let next = aggregate_of(&guard);
    WatchChange {
      existed: true,
      prev,
      next,
    }
  }

  pub fn unregister(&self, token: Uuid) -> WatchChange {
    let mut guard = self.inner.write().expect("geo watchers poisoned");
    let prev = aggregate_of(&guard);
    let existed = guard.remove(&token).is_some();
    let next = aggregate_of(&guard);
    WatchChange { existed, prev, next }
  }

  pub fn drain_for_owner(&self, owner: SocketAddr) -> WatchChange {
    let mut guard = self.inner.write().expect("geo watchers poisoned");
    let prev = aggregate_of(&guard);
    let drained: Vec<Uuid> = guard
      .iter()
      .filter_map(|(id, w)| (w.owner == owner).then_some(*id))
      .collect();
    let existed = !drained.is_empty();
    for id in drained {
      guard.remove(&id);
    }
    let next = aggregate_of(&guard);
    WatchChange { existed, prev, next }
  }

  pub fn owners(&self) -> Vec<SocketAddr> {
    let guard = self.inner.read().expect("geo watchers poisoned");
    let mut owners: Vec<SocketAddr> = guard.values().map(|w| w.owner).collect();
    owners.sort();
    owners.dedup();
    owners
  }
}

fn aggregate_of(map: &HashMap<Uuid, Watcher>) -> Option<WatchAggregate> {
  let mut iter = map.values();
  let first = iter.next()?;
  let mut accuracy = first.accuracy;
  let mut min_interval_ms = first.min_interval_ms;
  for w in iter {
    if matches!(w.accuracy, GeoAccuracy::Fine) {
      accuracy = GeoAccuracy::Fine;
    }
    if w.min_interval_ms < min_interval_ms {
      min_interval_ms = w.min_interval_ms;
    }
  }
  Some(WatchAggregate {
    accuracy,
    min_interval_ms,
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  fn addr(s: &str) -> SocketAddr {
    s.parse().unwrap()
  }

  #[test]
  fn first_register_emits_aggregate() {
    let w = GeoWatchers::new();
    let change = w.register(Uuid::now_v7(), addr("127.0.0.1:1"), GeoAccuracy::Coarse, 1000);
    assert_eq!(change.prev, None);
    assert_eq!(
      change.next,
      Some(WatchAggregate {
        accuracy: GeoAccuracy::Coarse,
        min_interval_ms: 1000,
      })
    );
  }

  #[test]
  fn fine_upgrades_aggregate() {
    let w = GeoWatchers::new();
    w.register(Uuid::now_v7(), addr("127.0.0.1:1"), GeoAccuracy::Coarse, 5000);
    let change = w.register(Uuid::now_v7(), addr("127.0.0.1:2"), GeoAccuracy::Fine, 10000);
    assert_eq!(
      change.prev,
      Some(WatchAggregate {
        accuracy: GeoAccuracy::Coarse,
        min_interval_ms: 5000,
      })
    );
    assert_eq!(
      change.next,
      Some(WatchAggregate {
        accuracy: GeoAccuracy::Fine,
        min_interval_ms: 5000,
      })
    );
  }

  #[test]
  fn smaller_interval_wins() {
    let w = GeoWatchers::new();
    w.register(Uuid::now_v7(), addr("127.0.0.1:1"), GeoAccuracy::Coarse, 5000);
    let change = w.register(Uuid::now_v7(), addr("127.0.0.1:2"), GeoAccuracy::Coarse, 1000);
    assert_eq!(change.next.unwrap().min_interval_ms, 1000);
  }

  #[test]
  fn unregister_last_returns_none_next() {
    let w = GeoWatchers::new();
    let token = Uuid::now_v7();
    w.register(token, addr("127.0.0.1:1"), GeoAccuracy::Fine, 1000);
    let change = w.unregister(token);
    assert!(change.existed);
    assert!(change.prev.is_some());
    assert_eq!(change.next, None);
  }

  #[test]
  fn unregister_unknown_is_noop() {
    let w = GeoWatchers::new();
    w.register(Uuid::now_v7(), addr("127.0.0.1:1"), GeoAccuracy::Fine, 1000);
    let change = w.unregister(Uuid::now_v7());
    assert!(!change.existed);
    assert_eq!(change.prev, change.next);
  }

  #[test]
  fn drain_for_owner_clears_only_that_owner() {
    let w = GeoWatchers::new();
    let alice = addr("127.0.0.1:1");
    let bob = addr("127.0.0.1:2");
    w.register(Uuid::now_v7(), alice, GeoAccuracy::Fine, 1000);
    w.register(Uuid::now_v7(), alice, GeoAccuracy::Fine, 5000);
    w.register(Uuid::now_v7(), bob, GeoAccuracy::Coarse, 3000);

    let change = w.drain_for_owner(alice);
    assert!(change.existed);
    assert_eq!(
      change.next,
      Some(WatchAggregate {
        accuracy: GeoAccuracy::Coarse,
        min_interval_ms: 3000,
      })
    );
    assert_eq!(w.owners(), vec![bob]);
  }

  #[test]
  fn owners_dedups_across_tokens() {
    let w = GeoWatchers::new();
    let a = addr("127.0.0.1:1");
    w.register(Uuid::now_v7(), a, GeoAccuracy::Fine, 1000);
    w.register(Uuid::now_v7(), a, GeoAccuracy::Coarse, 5000);
    assert_eq!(w.owners(), vec![a]);
  }
}
