//! Per-scope authority registry: which companion-supplied surface
//! ("NowPlayingMetadata", "NowPlayingPlayback", future scopes) is
//! currently authoritative, and how recently the companion refreshed
//! that claim.
//!
//! Read-mostly: every NowPlayingUpdate merge consults the registry,
//! companions write occasionally. `std::sync::RwLock<HashMap>` is the
//! right shape. Fast reads, no `await` between merge call sites.
//!
//! Stale claims fall back automatically. A claim that hasn't been
//! refreshed within `STALE_TIMEOUT` is treated as released on the next
//! `is_authoritative` query - covers companion crashes, BT blips that
//! drop a Release in flight, and OS-suspended companions.

use std::{
  collections::HashMap,
  sync::{Arc, RwLock},
  time::{Duration, Instant},
};

use libbridgething::gateway::CompanionAuthorityScope;

pub const STALE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Default)]
pub struct AuthorityRegistry {
  inner: Arc<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
  scopes: RwLock<HashMap<CompanionAuthorityScope, Instant>>,
}

impl AuthorityRegistry {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn claim(&self, scope: CompanionAuthorityScope) {
    let mut guard = self.inner.scopes.write().expect("authority lock poisoned");
    guard.insert(scope, Instant::now());
  }

  pub fn release(&self, scope: CompanionAuthorityScope) {
    let mut guard = self.inner.scopes.write().expect("authority lock poisoned");
    guard.remove(&scope);
  }

  pub fn drop_all(&self) {
    let mut guard = self.inner.scopes.write().expect("authority lock poisoned");
    guard.clear();
  }

  pub fn is_authoritative(&self, scope: CompanionAuthorityScope) -> bool {
    let guard = self.inner.scopes.read().expect("authority lock poisoned");
    guard
      .get(&scope)
      .map(|claimed_at| claimed_at.elapsed() < STALE_TIMEOUT)
      .unwrap_or(false)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn claim_and_release_round_trip() {
    let r = AuthorityRegistry::new();
    assert!(!r.is_authoritative(CompanionAuthorityScope::NowPlayingMetadata));
    r.claim(CompanionAuthorityScope::NowPlayingMetadata);
    assert!(r.is_authoritative(CompanionAuthorityScope::NowPlayingMetadata));
    r.release(CompanionAuthorityScope::NowPlayingMetadata);
    assert!(!r.is_authoritative(CompanionAuthorityScope::NowPlayingMetadata));
  }

  #[test]
  fn drop_all_clears_every_scope() {
    let r = AuthorityRegistry::new();
    r.claim(CompanionAuthorityScope::NowPlayingMetadata);
    r.claim(CompanionAuthorityScope::NowPlayingPlayback);
    r.drop_all();
    assert!(!r.is_authoritative(CompanionAuthorityScope::NowPlayingMetadata));
    assert!(!r.is_authoritative(CompanionAuthorityScope::NowPlayingPlayback));
  }

  #[test]
  fn scopes_are_independent() {
    let r = AuthorityRegistry::new();
    r.claim(CompanionAuthorityScope::NowPlayingMetadata);
    assert!(r.is_authoritative(CompanionAuthorityScope::NowPlayingMetadata));
    assert!(!r.is_authoritative(CompanionAuthorityScope::NowPlayingPlayback));
  }
}
