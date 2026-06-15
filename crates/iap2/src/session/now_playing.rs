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
  position_subscribed: bool,
  subscribed_at: Option<Instant>,
  last_app_bundle: Option<String>,
}

impl NowPlayingFlow {
  pub(super) fn new(rx: mpsc::Receiver<NowPlayingCommand>, artwork_suppress_bundles: Vec<String>) -> Self {
    Self {
      state: NowPlayingState::Idle,
      rx,
      artwork_suppress_bundles,
      art_subscribed: false,
      position_subscribed: true,
      subscribed_at: None,
      last_app_bundle: None,
    }
  }

  pub(super) fn handles(msg_id: u16) -> bool {
    msg_id == NowPlayingUpdate::CSM_MSG_ID
  }

  /// Send `StartNowPlayingUpdates` if we haven't yet. Idempotent.
  pub(super) async fn ensure_subscribed(&mut self, link_command_tx: &mpsc::Sender<Iap2Command>) -> Result<()> {
    if matches!(self.state, NowPlayingState::Idle) {
      tracing::debug!("iap2 now-playing: sending StartNowPlayingUpdates");
      send_csm(
        StartNowPlayingUpdates::subscription(self.art_subscribed, self.position_subscribed),
        link_command_tx,
      )
      .await?;
      self.state = NowPlayingState::Subscribed;
      self.subscribed_at = Some(Instant::now());
    }
    Ok(())
  }

  /// Opt in/out of iOS FileTransfer cover art and the ~1Hz position delta based on the active
  /// app. When a companion-served app (e.g. Spotify) is playing we skip both iOS art and the
  /// position flood: the companion's dealer already supplies sized art and a live bar position.
  async fn reconcile_subscription(
    &mut self,
    delta_bundle: Option<&str>,
    companion_connected: bool,
    link_command_tx: &mpsc::Sender<Iap2Command>,
  ) -> Result<()> {
    if let Some(b) = delta_bundle.filter(|b| !b.is_empty())
      && self.last_app_bundle.as_deref() != Some(b) {
        self.last_app_bundle = Some(b.to_string());
      }
    let is_suppress = match self.last_app_bundle.as_deref() {
      Some(bundle) => self.artwork_suppress_bundles.iter().any(|b| b == bundle),
      None => return Ok(()),
    };
    let companion_serving = is_suppress && companion_connected;
    let desired_art = if !is_suppress {
      true
    } else if companion_connected {
      false
    } else {
      self.subscribed_at.is_some_and(|t| t.elapsed() >= ART_SETTLE)
    };
    let desired_position = !companion_serving;
    if desired_art == self.art_subscribed && desired_position == self.position_subscribed {
      return Ok(());
    }
    self.art_subscribed = desired_art;
    self.position_subscribed = desired_position;
    tracing::debug!(
      bundle = self.last_app_bundle.as_deref().unwrap_or_default(),
      artwork = desired_art,
      position = desired_position,
      "iap2 now-playing: changing subscription"
    );
    send_csm(
      StartNowPlayingUpdates::subscription(desired_art, desired_position),
      link_command_tx,
    )
    .await?;
    Ok(())
  }

  /// Reconcile the subscription when companion presence changes, independent of an inbound delta.
  /// iOS stops emitting position deltas once position is suppressed, so a companion drop would
  /// otherwise never re-enable the iAP2 bar (no delta arrives to trigger `handle`).
  pub(super) async fn reconcile_companion(
    &mut self,
    companion_connected: bool,
    link_command_tx: &mpsc::Sender<Iap2Command>,
  ) -> Result<()> {
    if !matches!(self.state, NowPlayingState::Subscribed) {
      return Ok(());
    }
    self.reconcile_subscription(None, companion_connected, link_command_tx).await
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
      .reconcile_subscription(app_bundle.as_deref(), companion_connected, link_command_tx)
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
    assert!(f.position_subscribed, "default subscription carries position");

    // companion attached for the rest of this block
    let conn = true;

    // a non-companion app starts playing -> opt into iAP2 art (one re-subscribe)
    f.reconcile_subscription(Some("com.google.ios.youtube"), conn, &link_tx)
      .await
      .unwrap();
    assert!(f.art_subscribed);
    assert!(f.position_subscribed, "non-companion app keeps iAP2 position");
    assert!(link_rx.try_recv().is_ok(), "re-subscribed when artwork turned on");

    // same app again -> no redundant re-subscribe
    f.reconcile_subscription(Some("com.google.ios.youtube"), conn, &link_tx)
      .await
      .unwrap();
    assert!(link_rx.try_recv().is_err());

    // a position-only delta carries no bundle -> leave state untouched
    f.reconcile_subscription(None, conn, &link_tx).await.unwrap();
    assert!(f.art_subscribed);
    assert!(link_rx.try_recv().is_err());

    // iOS idle/transition sentinel (empty bundle) must not flap the subscription
    f.reconcile_subscription(Some(""), conn, &link_tx).await.unwrap();
    assert!(f.art_subscribed, "empty bundle leaves artwork state untouched");
    assert!(link_rx.try_recv().is_err(), "empty bundle triggers no re-subscribe");

    // Spotify active WITH companion -> drop both iAP2 art and position (one re-subscribe)
    f.reconcile_subscription(Some("com.spotify.client"), conn, &link_tx)
      .await
      .unwrap();
    assert!(!f.art_subscribed);
    assert!(
      !f.position_subscribed,
      "companion serving Spotify drops the iAP2 position flood"
    );
    assert!(link_rx.try_recv().is_ok(), "re-subscribed when artwork turned off");

    // Spotify still active -> no redundant re-subscribe
    f.reconcile_subscription(Some("com.spotify.client"), conn, &link_tx)
      .await
      .unwrap();
    assert!(link_rx.try_recv().is_err());
  }

  #[tokio::test]
  async fn position_only_deltas_suppress_after_companion_attaches_post_announce() {
    let (_np_tx, np_rx) = mpsc::channel(4);
    let (link_tx, mut link_rx) = mpsc::channel(8);
    let mut f = NowPlayingFlow::new(np_rx, vec!["com.spotify.client".to_string()]);
    f.state = NowPlayingState::Subscribed;

    // spotify announces foreground before the companion ea stream is up (the connect race).
    f.subscribed_at = Some(Instant::now() - ART_SETTLE - Duration::from_millis(1));
    f.reconcile_subscription(Some("com.spotify.client"), false, &link_tx)
      .await
      .unwrap();
    assert!(f.position_subscribed, "no companion yet -> iAP2 still owns the bar");
    while link_rx.try_recv().is_ok() {} // drain art reconcile from the settle elapse

    // companion attaches, but ios now only emits position-only deltas (no app_bundle). the flow
    // must remember spotify is foreground and drop the position flood off the shared link anyway.
    f.reconcile_subscription(None, true, &link_tx).await.unwrap();
    assert!(
      !f.position_subscribed,
      "position-only delta still drops the flood via the remembered bundle"
    );
    assert!(link_rx.try_recv().is_ok(), "re-subscribed to stop the position flood");

    // companion drops while suppressed -> ios is no longer sending position deltas, so the
    // companion-transition hook must re-enable position without an inbound delta.
    f.reconcile_companion(false, &link_tx).await.unwrap();
    assert!(
      f.position_subscribed,
      "companion drop re-enables the iAP2 bar even with no delta to trigger it"
    );
    assert!(link_rx.try_recv().is_ok(), "re-subscribed to restore position on companion drop");
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
    f.reconcile_subscription(Some("com.spotify.client"), false, &link_tx)
      .await
      .unwrap();
    assert!(
      !f.art_subscribed,
      "suppress-bundle holds art off during the settle window"
    );
    assert!(link_rx.try_recv().is_err(), "no re-subscribe inside the settle window");

    // settle window elapsed with still no companion -> iAP2 art is the only source.
    f.subscribed_at = Some(Instant::now() - ART_SETTLE - Duration::from_millis(1));
    f.reconcile_subscription(Some("com.spotify.client"), false, &link_tx)
      .await
      .unwrap();
    assert!(f.art_subscribed, "after settle, no-companion Spotify gets iAP2 art");
    assert!(link_rx.try_recv().is_ok());

    // companion attaches -> drop the now-redundant iAP2 art
    f.reconcile_subscription(Some("com.spotify.client"), true, &link_tx)
      .await
      .unwrap();
    assert!(!f.art_subscribed);
    assert!(link_rx.try_recv().is_ok());
  }

  #[tokio::test]
  async fn position_drops_only_when_a_companion_serves_the_suppress_bundle() {
    let (_np_tx, np_rx) = mpsc::channel(4);
    let (link_tx, mut link_rx) = mpsc::channel(8);
    let mut f = NowPlayingFlow::new(np_rx, vec!["com.spotify.client".to_string()]);
    f.state = NowPlayingState::Subscribed;

    // Spotify foreground but no companion attached yet -> iAP2 still owns the bar, keep position.
    f.subscribed_at = Some(Instant::now() - ART_SETTLE - Duration::from_millis(1));
    f.reconcile_subscription(Some("com.spotify.client"), false, &link_tx)
      .await
      .unwrap();
    assert!(
      f.position_subscribed,
      "no companion -> iAP2 position stays the bar source"
    );
    let _ = link_rx.try_recv(); // drain the art-on re-subscribe from the settle elapse

    // companion attaches while Spotify is foreground -> companion drives the bar, drop position.
    f.reconcile_subscription(Some("com.spotify.client"), true, &link_tx)
      .await
      .unwrap();
    assert!(!f.position_subscribed);
    assert!(link_rx.try_recv().is_ok(), "re-subscribed when position turned off");

    // companion still present but a non-suppress app comes foreground -> iAP2 owns it again.
    f.reconcile_subscription(Some("com.google.ios.youtube"), true, &link_tx)
      .await
      .unwrap();
    assert!(
      f.position_subscribed,
      "non-suppress foreground app re-enables iAP2 position"
    );
    assert!(link_rx.try_recv().is_ok());
  }
}
