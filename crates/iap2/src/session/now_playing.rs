//! NowPlaying flow: subscribes to the iPhone's NowPlaying surface
//! after identification reaches Accepted, then translates each
//! inbound `NowPlayingUpdate` (CSM `0x5001`) into a session event.
//! Also handles outbound `SetNowPlayingInformation` (CSM `0x5003`)
//! commands that the daemon's `TransportController` issues for scrub
//! and queue-jump verbs.
//!
//! `ensure_subscribed` sends `StartNowPlayingUpdates` exactly once per
//! session - the iPhone keeps the subscription for the life of the
//! link. Subsequent calls are no-ops, so the session is free to call
//! it after every CSM dispatch as a "kick if needed" check.

use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use super::{SessionEvent, emit, send_csm};
use crate::{
  csm::{
    CsmFrame,
    now_playing::{NowPlayingUpdate, SetNowPlayingInformation, StartNowPlayingUpdates},
  },
  error::Result,
  link::Iap2Command,
};

/// A companion EA stream attaches a few seconds after the first NowPlaying deltas. For a
/// suppress-bundle with no companion yet, hold iAP2 art off this long so a still-attaching
/// companion does not trigger a one-shot 193 KiB art flood at connect.
const ART_SETTLE: Duration = Duration::from_secs(4);

/// One outbound NowPlaying control message; the flow turns it into a `SetNowPlayingInformation`
/// CSM. The `set_elapsed_time_available` gate is enforced upstream; this flow trusts callers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NowPlayingCommand {
  pub elapsed_time_ms: Option<u32>,
  pub queue_index: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NowPlayingState {
  /// Identification has not yet reached Accepted; nothing to do.
  Idle,
  /// `StartNowPlayingUpdates` has been sent; deltas may arrive at any time.
  Subscribed,
}

pub(super) struct NowPlayingFlow {
  state: NowPlayingState,
  rx: mpsc::Receiver<NowPlayingCommand>,
  artwork_suppress_bundles: Vec<String>,
  art_subscribed: bool,
  subscribed_at: Option<Instant>,
}

impl NowPlayingFlow {
  pub(super) fn new(rx: mpsc::Receiver<NowPlayingCommand>, artwork_suppress_bundles: Vec<String>) -> Self {
    Self {
      state: NowPlayingState::Idle,
      rx,
      artwork_suppress_bundles,
      art_subscribed: false,
      subscribed_at: None,
    }
  }

  pub(super) fn handles(msg_id: u16) -> bool {
    msg_id == NowPlayingUpdate::CSM_MSG_ID
  }

  /// Send `StartNowPlayingUpdates` if we haven't yet. Idempotent.
  pub(super) async fn ensure_subscribed(&mut self, link_command_tx: &mpsc::Sender<Iap2Command>) -> Result<()> {
    if matches!(self.state, NowPlayingState::Idle) {
      tracing::debug!("iap2 now-playing: sending StartNowPlayingUpdates");
      send_csm(StartNowPlayingUpdates::subscription(self.art_subscribed), link_command_tx).await?;
      self.state = NowPlayingState::Subscribed;
      self.subscribed_at = Some(Instant::now());
    }
    Ok(())
  }

  /// Opt in/out of iOS FileTransfer cover art based on the active app. When a
  /// companion-served app (e.g. Spotify) is playing we skip iOS art entirely;
  async fn reconcile_artwork(
    &mut self,
    app_bundle: Option<&str>,
    companion_connected: bool,
    link_command_tx: &mpsc::Sender<Iap2Command>,
  ) -> Result<()> {
    let Some(bundle) = app_bundle.filter(|b| !b.is_empty()) else {
      return Ok(());
    };
    let is_suppress = self.artwork_suppress_bundles.iter().any(|b| b == bundle);
    let desired = if !is_suppress {
      true
    } else if companion_connected {
      false
    } else {
      self.subscribed_at.is_some_and(|t| t.elapsed() >= ART_SETTLE)
    };
    if desired == self.art_subscribed {
      return Ok(());
    }
    self.art_subscribed = desired;
    tracing::debug!(bundle, artwork = desired, "iap2 now-playing: changing artwork subscription");
    send_csm(StartNowPlayingUpdates::subscription(desired), link_command_tx).await?;
    Ok(())
  }

  /// Process one NowPlaying-range CSM. Always returns `Ok(None)`; NowPlaying has no terminal
  /// failure state of its own.
  pub(super) async fn handle(
    &mut self,
    frame: CsmFrame,
    companion_connected: bool,
    link_command_tx: &mpsc::Sender<Iap2Command>,
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
    let app_bundle = update.playback.as_ref().and_then(|p| p.app_bundle.clone());
    emit(session_events_tx, SessionEvent::NowPlayingUpdate(update)).await;
    self
      .reconcile_artwork(app_bundle.as_deref(), companion_connected, link_command_tx)
      .await?;
    Ok(None)
  }

  /// Pull the next outbound command from the controller. `None` means the sender was dropped.
  pub(super) async fn recv(&mut self) -> Option<NowPlayingCommand> {
    self.rx.recv().await
  }

  /// Translate an outbound command into a `SetNowPlayingInformation`
  /// CSM. No-op when neither `elapsed_time_ms` nor `queue_index` is
  /// set (the `Default` value).
  pub(super) async fn handle_command(
    &mut self,
    cmd: NowPlayingCommand,
    link_command_tx: &mpsc::Sender<Iap2Command>,
  ) -> Result<()> {
    if !matches!(self.state, NowPlayingState::Subscribed) {
      tracing::warn!(
        ?cmd,
        "iap2 now-playing: command before StartNowPlayingUpdates; dropping"
      );
      return Ok(());
    }
    if cmd.elapsed_time_ms.is_none() && cmd.queue_index.is_none() {
      tracing::trace!("iap2 now-playing: empty NowPlayingCommand; ignoring");
      return Ok(());
    }
    let csm = SetNowPlayingInformation {
      elapsed_time_ms: cmd.elapsed_time_ms,
      queue_index: cmd.queue_index,
      queue_list_content_transfer_start_index: None,
    };
    tracing::debug!(?csm, "iap2 now-playing: sending SetNowPlayingInformation");
    send_csm(csm, link_command_tx).await
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn artwork_subscription_follows_active_app() {
    let (_np_tx, np_rx) = mpsc::channel(4);
    let (link_tx, mut link_rx) = mpsc::channel(8);
    let mut f = NowPlayingFlow::new(np_rx, vec!["com.spotify.client".to_string()]);
    f.state = NowPlayingState::Subscribed;
    assert!(!f.art_subscribed, "default subscription carries no artwork");

    // companion attached for the rest of this block
    let conn = true;

    // a non-companion app starts playing -> opt into iAP2 art (one re-subscribe)
    f.reconcile_artwork(Some("com.google.ios.youtube"), conn, &link_tx).await.unwrap();
    assert!(f.art_subscribed);
    assert!(link_rx.try_recv().is_ok(), "re-subscribed when artwork turned on");

    // same app again -> no redundant re-subscribe
    f.reconcile_artwork(Some("com.google.ios.youtube"), conn, &link_tx).await.unwrap();
    assert!(link_rx.try_recv().is_err());

    // a position-only delta carries no bundle -> leave state untouched
    f.reconcile_artwork(None, conn, &link_tx).await.unwrap();
    assert!(f.art_subscribed);
    assert!(link_rx.try_recv().is_err());

    // iOS idle/transition sentinel (empty bundle) must not flap the subscription
    f.reconcile_artwork(Some(""), conn, &link_tx).await.unwrap();
    assert!(f.art_subscribed, "empty bundle leaves artwork state untouched");
    assert!(link_rx.try_recv().is_err(), "empty bundle triggers no re-subscribe");

    // Spotify active WITH companion -> drop iAP2 art (one re-subscribe)
    f.reconcile_artwork(Some("com.spotify.client"), conn, &link_tx).await.unwrap();
    assert!(!f.art_subscribed);
    assert!(link_rx.try_recv().is_ok(), "re-subscribed when artwork turned off");

    // Spotify still active -> no redundant re-subscribe
    f.reconcile_artwork(Some("com.spotify.client"), conn, &link_tx).await.unwrap();
    assert!(link_rx.try_recv().is_err());
  }

  #[tokio::test]
  async fn spotify_artwork_waits_out_settle_then_stays_on_with_no_companion() {
    let (_np_tx, np_rx) = mpsc::channel(4);
    let (link_tx, mut link_rx) = mpsc::channel(8);
    let mut f = NowPlayingFlow::new(np_rx, vec!["com.spotify.client".to_string()]);
    f.state = NowPlayingState::Subscribed;

    // within the settle window a companion may still be attaching: hold art off so a
    // late-attaching companion does not eat a one-shot art flood.
    f.subscribed_at = Some(Instant::now());
    f.reconcile_artwork(Some("com.spotify.client"), false, &link_tx).await.unwrap();
    assert!(!f.art_subscribed, "suppress-bundle holds art off during the settle window");
    assert!(link_rx.try_recv().is_err(), "no re-subscribe inside the settle window");

    // settle window elapsed with still no companion -> iAP2 art is the only source.
    f.subscribed_at = Some(Instant::now() - ART_SETTLE - Duration::from_millis(1));
    f.reconcile_artwork(Some("com.spotify.client"), false, &link_tx).await.unwrap();
    assert!(f.art_subscribed, "after settle, no-companion Spotify gets iAP2 art");
    assert!(link_rx.try_recv().is_ok());

    // companion attaches -> drop the now-redundant iAP2 art
    f.reconcile_artwork(Some("com.spotify.client"), true, &link_tx).await.unwrap();
    assert!(!f.art_subscribed);
    assert!(link_rx.try_recv().is_ok());
  }
}
