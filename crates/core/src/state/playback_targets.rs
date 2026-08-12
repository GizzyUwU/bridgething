use std::sync::{Arc, RwLock};

use libbridgething::{
  PlaybackTarget,
  client::{BridgeToClientPlayerMsgEvent, PlayerTargetsReply},
};

use crate::{bluetooth::Address, net::WireEventBus};

#[derive(Debug, Clone)]
pub struct PlaybackTargetStore {
  inner: Arc<RwLock<Owned>>,
  bus: WireEventBus,
}

#[derive(Debug, Default)]
struct Owned {
  targets: Vec<PlaybackTarget>,
  owner: Option<Address>,
}

impl PlaybackTargetStore {
  pub fn new(bus: WireEventBus) -> Self {
    Self {
      inner: Arc::new(RwLock::new(Owned::default())),
      bus,
    }
  }

  pub fn current(&self) -> PlayerTargetsReply {
    PlayerTargetsReply {
      targets: self
        .inner
        .read()
        .expect("playback targets lock poisoned")
        .targets
        .clone(),
    }
  }

  pub async fn apply_companion(&self, addr: Address, targets: Vec<PlaybackTarget>) -> Result<(), PlaybackTargetError> {
    self.replace(Some(addr), targets).await
  }

  pub async fn clear_companion(&self, addr: Address) -> Result<(), PlaybackTargetError> {
    if self.inner.read().expect("playback targets lock poisoned").owner != Some(addr) {
      return Ok(());
    }
    self.replace(None, Vec::new()).await
  }

  async fn replace(&self, owner: Option<Address>, targets: Vec<PlaybackTarget>) -> Result<(), PlaybackTargetError> {
    {
      let mut guard = self.inner.write().expect("playback targets lock poisoned");
      if guard.targets == targets && guard.owner == owner {
        return Ok(());
      }
      guard.targets = targets;
      guard.owner = owner;
    }
    self.broadcast_current().await
  }

  async fn broadcast_current(&self) -> Result<(), PlaybackTargetError> {
    let reply = self.current();
    self
      .bus
      .broadcast_event(BridgeToClientPlayerMsgEvent::TargetsChanged(reply))
      .await?;
    Ok(())
  }
}

#[derive(Debug, thiserror::Error)]
pub enum PlaybackTargetError {
  #[error(transparent)]
  WS(#[from] crate::net::WSError),
}

crate::impl_broadcast_failure_from!(PlaybackTargetError);

#[cfg(test)]
mod tests {
  use libbridgething::PlaybackTargetKind;

  use super::*;

  fn store() -> PlaybackTargetStore {
    let (client_man, _listener) = crate::net::create_client_manager();
    PlaybackTargetStore::new(WireEventBus::new(client_man))
  }

  fn target(id: &str, active: bool) -> PlaybackTarget {
    PlaybackTarget {
      id: id.to_string(),
      name: id.to_string(),
      kind: PlaybackTargetKind::Speaker,
      is_active: active,
      volume_percent: Some(40),
    }
  }

  #[tokio::test]
  async fn starts_empty() {
    assert!(store().current().targets.is_empty());
  }

  fn addr(last: u8) -> Address {
    Address([0xAA, 0, 0, 0, 0, last])
  }

  #[tokio::test]
  async fn apply_then_read_back() {
    let s = store();
    let _ = s.apply_companion(addr(1), vec![target("kitchen", true)]).await;
    let targets = s.current().targets;
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].id, "kitchen");
    assert!(targets[0].is_active);
  }

  #[tokio::test]
  async fn a_full_replacement_drops_endpoints_that_went_away() {
    let s = store();
    let _ = s
      .apply_companion(addr(1), vec![target("kitchen", true), target("desk", false)])
      .await;
    let _ = s.apply_companion(addr(1), vec![target("desk", true)]).await;
    let targets = s.current().targets;
    assert_eq!(targets.len(), 1, "list is a replacement, not a merge");
    assert_eq!(targets[0].id, "desk");
  }

  #[tokio::test]
  async fn clear_empties_the_list() {
    let s = store();
    let _ = s.apply_companion(addr(1), vec![target("kitchen", true)]).await;
    let _ = s.clear_companion(addr(1)).await;
    assert!(s.current().targets.is_empty());
  }

  #[tokio::test]
  async fn a_peer_companion_leaving_does_not_clear_the_list() {
    let s = store();
    let _ = s.apply_companion(addr(1), vec![target("kitchen", true)]).await;
    let _ = s.clear_companion(addr(2)).await;
    assert_eq!(
      s.current().targets.len(),
      1,
      "a peer companion cleared someone else's targets"
    );
  }
}
