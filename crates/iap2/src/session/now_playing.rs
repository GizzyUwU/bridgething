//! NowPlaying flow: subscribes to the iPhone's NowPlaying surface
//! after identification reaches Accepted, then translates each
//! inbound `NowPlayingUpdate` (CSM `0x5001`) into a session event.
//!
//! `ensure_subscribed` sends `StartNowPlayingUpdates` exactly once per
//! session - the iPhone keeps the subscription for the life of the
//! link. Subsequent calls are no-ops, so the session is free to call
//! it after every CSM dispatch as a "kick if needed" check.

use tokio::sync::mpsc;

use super::{SessionEvent, emit, send_csm};
use crate::{
  csm::{
    CsmFrame,
    now_playing::{NowPlayingUpdate, StartNowPlayingUpdates},
  },
  error::Result,
  link::Iap2Command,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NowPlayingState {
  /// Identification has not yet reached Accepted; nothing to do.
  Idle,
  /// `StartNowPlayingUpdates` has been sent; deltas may arrive at any time.
  Subscribed,
}

pub(super) struct NowPlayingFlow {
  state: NowPlayingState,
}

impl NowPlayingFlow {
  pub(super) fn new() -> Self {
    Self {
      state: NowPlayingState::Idle,
    }
  }

  pub(super) fn handles(msg_id: u16) -> bool {
    msg_id == NowPlayingUpdate::CSM_MSG_ID
  }

  /// Send `StartNowPlayingUpdates` if we haven't yet. Idempotent;
  /// callers may invoke after every dispatch as a cheap "kick once
  /// identification is accepted" check.
  pub(super) async fn ensure_subscribed(&mut self, link_command_tx: &mpsc::Sender<Iap2Command>) -> Result<()> {
    if matches!(self.state, NowPlayingState::Idle) {
      tracing::debug!("iap2 now-playing: sending StartNowPlayingUpdates");
      send_csm(StartNowPlayingUpdates::standard(), link_command_tx).await?;
      self.state = NowPlayingState::Subscribed;
    }
    Ok(())
  }

  /// Process one NowPlaying-range CSM. Always returns `Ok(None)` -
  /// NowPlaying has no terminal failure state of its own; if the
  /// iPhone stops pushing updates the link itself falls over and the
  /// session emits `LinkDown` from there.
  pub(super) async fn handle(
    &mut self,
    frame: CsmFrame,
    session_events_tx: &mpsc::Sender<SessionEvent>,
  ) -> Result<Option<SessionEvent>> {
    if frame.msg_id != NowPlayingUpdate::CSM_MSG_ID {
      return Ok(None);
    }
    let update: NowPlayingUpdate = frame.try_into()?;
    if matches!(self.state, NowPlayingState::Idle) {
      tracing::warn!("iap2 now-playing: received update before subscribing; surfacing anyway");
    }
    tracing::trace!(?update, "iap2 now-playing: delta received");
    emit(session_events_tx, SessionEvent::NowPlayingUpdate(update)).await;
    Ok(None)
  }
}
