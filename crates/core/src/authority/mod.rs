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
//!
//! The now-playing scopes are the exception: they hold indefinitely once
//! claimed. The companion claims them once when its app becomes the
//! current player, and the dealer pushes only on change, so there is no
//! periodic refresh to ride a staleness clock. They drop on explicit
//! release (sign-out), on the companion-disconnect hook (`drop_all`), or
//! get arbitrated away by the player's iAP2 app-bundle gate when another
//! app takes the foreground. The companion declares its app bundle on the
//! claim so that gate can compare it against iAP2's foreground signal.

use std::{
  collections::HashMap,
  sync::{Arc, RwLock},
  time::{Duration, Instant},
};

use bridgething_iap2::NowPlayingAuthorityState;
use libbridgething::CompanionAuthorityScope;
use tokio::sync::watch;

pub const STALE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Default)]
pub struct AuthorityRegistry {
  inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
  scopes: RwLock<HashMap<CompanionAuthorityScope, Instant>>,
  companion_app_bundle: RwLock<Option<String>>,
  now_playing: watch::Sender<NowPlayingAuthorityState>,
}

impl Default for Inner {
  fn default() -> Self {
    let (now_playing, _) = watch::channel(NowPlayingAuthorityState::default());
    Inner {
      scopes: RwLock::new(HashMap::new()),
      companion_app_bundle: RwLock::new(None),
      now_playing,
    }
  }
}

fn scope_holds_indefinitely(scope: CompanionAuthorityScope) -> bool {
  matches!(
    scope,
    CompanionAuthorityScope::NowPlayingMetadata
      | CompanionAuthorityScope::NowPlayingPlayback
      | CompanionAuthorityScope::Volume
  )
}

impl AuthorityRegistry {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn claim(&self, scope: CompanionAuthorityScope) {
    let mut guard = self.inner.scopes.write().expect("authority lock poisoned");
    guard.insert(scope, Instant::now());
    self.publish_now_playing(&guard);
  }

  pub fn release(&self, scope: CompanionAuthorityScope) {
    let mut guard = self.inner.scopes.write().expect("authority lock poisoned");
    guard.remove(&scope);
    self.publish_now_playing(&guard);
  }

  pub fn drop_all(&self) {
    let mut guard = self.inner.scopes.write().expect("authority lock poisoned");
    guard.clear();
    self.publish_now_playing(&guard);
    drop(guard);
    *self
      .inner
      .companion_app_bundle
      .write()
      .expect("authority lock poisoned") = None;
  }

  pub fn now_playing_subscription_rx(&self) -> watch::Receiver<NowPlayingAuthorityState> {
    self.inner.now_playing.subscribe()
  }

  fn publish_now_playing(&self, scopes: &HashMap<CompanionAuthorityScope, Instant>) {
    let state = NowPlayingAuthorityState {
      companion_metadata: scopes.contains_key(&CompanionAuthorityScope::NowPlayingMetadata),
      companion_playback: scopes.contains_key(&CompanionAuthorityScope::NowPlayingPlayback),
    };
    self.inner.now_playing.send_if_modified(|cur| {
      let changed = *cur != state;
      *cur = state;
      changed
    });
  }

  pub fn set_companion_app_bundle(&self, bundle: Option<String>) {
    if let Some(bundle) = bundle {
      *self
        .inner
        .companion_app_bundle
        .write()
        .expect("authority lock poisoned") = Some(bundle);
    }
  }

  pub fn companion_app_bundle(&self) -> Option<String> {
    self
      .inner
      .companion_app_bundle
      .read()
      .expect("authority lock poisoned")
      .clone()
  }

  pub fn is_authoritative(&self, scope: CompanionAuthorityScope) -> bool {
    let guard = self.inner.scopes.read().expect("authority lock poisoned");
    guard
      .get(&scope)
      .map(|claimed_at| scope_holds_indefinitely(scope) || claimed_at.elapsed() < STALE_TIMEOUT)
      .unwrap_or(false)
  }

  pub fn live_scopes(&self) -> Vec<CompanionAuthorityScope> {
    let guard = self.inner.scopes.read().expect("authority lock poisoned");
    guard
      .iter()
      .filter(|(scope, claimed_at)| scope_holds_indefinitely(**scope) || claimed_at.elapsed() < STALE_TIMEOUT)
      .map(|(scope, _)| *scope)
      .collect()
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

  #[test]
  fn now_playing_subscription_mirrors_scope_claims() {
    let r = AuthorityRegistry::new();
    let rx = r.now_playing_subscription_rx();
    assert_eq!(*rx.borrow(), NowPlayingAuthorityState::default());

    r.claim(CompanionAuthorityScope::NowPlayingMetadata);
    assert_eq!(
      *rx.borrow(),
      NowPlayingAuthorityState {
        companion_metadata: true,
        companion_playback: false,
      }
    );

    r.claim(CompanionAuthorityScope::NowPlayingPlayback);
    assert!(rx.borrow().companion_playback);

    // a non-now-playing scope must not move the now-playing signal.
    r.claim(CompanionAuthorityScope::Volume);
    assert!(rx.borrow().companion_metadata && rx.borrow().companion_playback);

    r.drop_all();
    assert_eq!(*rx.borrow(), NowPlayingAuthorityState::default());
  }

  #[test]
  fn volume_holds_past_stale_timeout() {
    // the companion claims volume once and only re-sends on a real change, so it must hold like the
    // now-playing scopes rather than expire after STALE_TIMEOUT.
    assert!(scope_holds_indefinitely(CompanionAuthorityScope::Volume));
    let r = AuthorityRegistry::new();
    r.inner
      .scopes
      .write()
      .unwrap()
      .insert(CompanionAuthorityScope::Volume, Instant::now() - STALE_TIMEOUT * 4);
    assert!(r.is_authoritative(CompanionAuthorityScope::Volume));
  }
}
