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

const ART_SETTLE: Duration = Duration::from_secs(4);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NowPlayingAuthorityState {
  pub companion_metadata: bool,
  pub companion_playback: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NowPlayingCommand {
  pub elapsed_time_ms: Option<u32>,
  pub queue_index: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NowPlayingState {
  Idle,
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

  pub(super) fn reset(&mut self) {
    self.state = NowPlayingState::Idle;
    self.subscribed_at = None;
    self.last_app_bundle = None;
  }

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

  async fn reconcile_subscription(
    &mut self,
    delta_bundle: Option<&str>,
    authority: NowPlayingAuthorityState,
    link_command_tx: &mpsc::Sender<Iap2Command>,
  ) -> Result<()> {
    if let Some(b) = delta_bundle.filter(|b| !b.is_empty())
      && self.last_app_bundle.as_deref() != Some(b)
    {
      self.last_app_bundle = Some(b.to_string());
    }
    let is_suppress = match self.last_app_bundle.as_deref() {
      Some(bundle) => self.artwork_suppress_bundles.iter().any(|b| b == bundle),
      None => return Ok(()),
    };
    let desired_art = if !is_suppress {
      true
    } else if authority.companion_metadata {
      false
    } else {
      self.subscribed_at.is_some_and(|t| t.elapsed() >= ART_SETTLE)
    };
    let desired_position = !(is_suppress && authority.companion_playback);
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

  pub(super) async fn reconcile_companion(
    &mut self,
    authority: NowPlayingAuthorityState,
    link_command_tx: &mpsc::Sender<Iap2Command>,
  ) -> Result<()> {
    if !matches!(self.state, NowPlayingState::Subscribed) {
      return Ok(());
    }
    self.reconcile_subscription(None, authority, link_command_tx).await
  }

  pub(super) async fn handle(
    &mut self,
    frame: CsmFrame,
    authority: NowPlayingAuthorityState,
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
      .reconcile_subscription(app_bundle.as_deref(), authority, link_command_tx)
      .await?;
    Ok(None)
  }

  pub(super) async fn recv(&mut self) -> Option<NowPlayingCommand> {
    self.rx.recv().await
  }

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

  const SERVING: NowPlayingAuthorityState = NowPlayingAuthorityState {
    companion_metadata: true,
    companion_playback: true,
  };
  const ABSENT: NowPlayingAuthorityState = NowPlayingAuthorityState {
    companion_metadata: false,
    companion_playback: false,
  };

  #[tokio::test]
  async fn artwork_subscription_follows_active_app() {
    let (_np_tx, np_rx) = mpsc::channel(4);
    let (link_tx, mut link_rx) = mpsc::channel(8);
    let mut f = NowPlayingFlow::new(np_rx, vec!["com.spotify.client".to_string()]);
    f.state = NowPlayingState::Subscribed;
    assert!(!f.art_subscribed, "default subscription carries no artwork");
    assert!(f.position_subscribed, "default subscription carries position");

    f.reconcile_subscription(Some("com.google.ios.youtube"), SERVING, &link_tx)
      .await
      .unwrap();
    assert!(f.art_subscribed);
    assert!(f.position_subscribed, "non-companion app keeps iAP2 position");
    assert!(link_rx.try_recv().is_ok(), "re-subscribed when artwork turned on");

    f.reconcile_subscription(Some("com.google.ios.youtube"), SERVING, &link_tx)
      .await
      .unwrap();
    assert!(link_rx.try_recv().is_err());

    f.reconcile_subscription(None, SERVING, &link_tx).await.unwrap();
    assert!(f.art_subscribed);
    assert!(link_rx.try_recv().is_err());

    f.reconcile_subscription(Some(""), SERVING, &link_tx).await.unwrap();
    assert!(f.art_subscribed, "empty bundle leaves artwork state untouched");
    assert!(link_rx.try_recv().is_err(), "empty bundle triggers no re-subscribe");

    f.reconcile_subscription(Some("com.spotify.client"), SERVING, &link_tx)
      .await
      .unwrap();
    assert!(!f.art_subscribed);
    assert!(
      !f.position_subscribed,
      "companion serving Spotify drops the iAP2 position flood"
    );
    assert!(link_rx.try_recv().is_ok(), "re-subscribed when artwork turned off");

    f.reconcile_subscription(Some("com.spotify.client"), SERVING, &link_tx)
      .await
      .unwrap();
    assert!(link_rx.try_recv().is_err());
  }

  #[tokio::test]
  async fn position_only_deltas_suppress_after_companion_claims_post_announce() {
    let (_np_tx, np_rx) = mpsc::channel(4);
    let (link_tx, mut link_rx) = mpsc::channel(8);
    let mut f = NowPlayingFlow::new(np_rx, vec!["com.spotify.client".to_string()]);
    f.state = NowPlayingState::Subscribed;

    f.subscribed_at = Some(Instant::now() - ART_SETTLE - Duration::from_millis(1));
    f.reconcile_subscription(Some("com.spotify.client"), ABSENT, &link_tx)
      .await
      .unwrap();
    assert!(
      f.position_subscribed,
      "no companion authority yet -> iAP2 still owns the bar"
    );
    while link_rx.try_recv().is_ok() {} // drain art reconcile from the settle elapse

    f.reconcile_subscription(None, SERVING, &link_tx).await.unwrap();
    assert!(
      !f.position_subscribed,
      "position-only delta still drops the flood via the remembered bundle"
    );
    assert!(link_rx.try_recv().is_ok(), "re-subscribed to stop the position flood");

    f.reconcile_companion(ABSENT, &link_tx).await.unwrap();
    assert!(
      f.position_subscribed,
      "authority release re-enables the iAP2 bar even with no delta to trigger it"
    );
    assert!(
      link_rx.try_recv().is_ok(),
      "re-subscribed to restore position on authority release"
    );
  }

  #[tokio::test]
  async fn spotify_artwork_waits_out_settle_then_stays_on_without_authority() {
    let (_np_tx, np_rx) = mpsc::channel(4);
    let (link_tx, mut link_rx) = mpsc::channel(8);
    let mut f = NowPlayingFlow::new(np_rx, vec!["com.spotify.client".to_string()]);
    f.state = NowPlayingState::Subscribed;

    f.subscribed_at = Some(Instant::now());
    f.reconcile_subscription(Some("com.spotify.client"), ABSENT, &link_tx)
      .await
      .unwrap();
    assert!(
      !f.art_subscribed,
      "suppress-bundle holds art off during the settle window"
    );
    assert!(link_rx.try_recv().is_err(), "no re-subscribe inside the settle window");

    f.subscribed_at = Some(Instant::now() - ART_SETTLE - Duration::from_millis(1));
    f.reconcile_subscription(Some("com.spotify.client"), ABSENT, &link_tx)
      .await
      .unwrap();
    assert!(f.art_subscribed, "after settle, authority-less Spotify gets iAP2 art");
    assert!(link_rx.try_recv().is_ok());

    f.reconcile_subscription(Some("com.spotify.client"), SERVING, &link_tx)
      .await
      .unwrap();
    assert!(!f.art_subscribed);
    assert!(link_rx.try_recv().is_ok());
  }

  #[tokio::test]
  async fn position_drops_only_when_a_companion_owns_the_suppress_bundle() {
    let (_np_tx, np_rx) = mpsc::channel(4);
    let (link_tx, mut link_rx) = mpsc::channel(8);
    let mut f = NowPlayingFlow::new(np_rx, vec!["com.spotify.client".to_string()]);
    f.state = NowPlayingState::Subscribed;

    f.subscribed_at = Some(Instant::now() - ART_SETTLE - Duration::from_millis(1));
    f.reconcile_subscription(Some("com.spotify.client"), ABSENT, &link_tx)
      .await
      .unwrap();
    assert!(
      f.position_subscribed,
      "no companion authority -> iAP2 position stays the bar source"
    );
    let _ = link_rx.try_recv(); // drain the art-on re-subscribe from the settle elapse

    f.reconcile_subscription(Some("com.spotify.client"), SERVING, &link_tx)
      .await
      .unwrap();
    assert!(!f.position_subscribed);
    assert!(link_rx.try_recv().is_ok(), "re-subscribed when position turned off");

    f.reconcile_subscription(Some("com.google.ios.youtube"), SERVING, &link_tx)
      .await
      .unwrap();
    assert!(
      f.position_subscribed,
      "non-suppress foreground app re-enables iAP2 position"
    );
    assert!(link_rx.try_recv().is_ok());
  }

  #[tokio::test]
  async fn art_and_position_gate_on_independent_scopes() {
    let (_np_tx, np_rx) = mpsc::channel(4);
    let (link_tx, mut link_rx) = mpsc::channel(8);
    let mut f = NowPlayingFlow::new(np_rx, vec!["com.spotify.client".to_string()]);
    f.state = NowPlayingState::Subscribed;
    f.subscribed_at = Some(Instant::now() - ART_SETTLE - Duration::from_millis(1));

    f.reconcile_subscription(Some("com.spotify.client"), ABSENT, &link_tx)
      .await
      .unwrap();
    assert!(f.art_subscribed && f.position_subscribed);
    while link_rx.try_recv().is_ok() {}

    let metadata_only = NowPlayingAuthorityState {
      companion_metadata: true,
      companion_playback: false,
    };
    f.reconcile_subscription(Some("com.spotify.client"), metadata_only, &link_tx)
      .await
      .unwrap();
    assert!(!f.art_subscribed, "metadata authority drops iAP2 art");
    assert!(f.position_subscribed, "no playback authority keeps the iAP2 bar");
    assert!(link_rx.try_recv().is_ok());

    let playback_only = NowPlayingAuthorityState {
      companion_metadata: false,
      companion_playback: true,
    };
    f.reconcile_subscription(Some("com.spotify.client"), playback_only, &link_tx)
      .await
      .unwrap();
    assert!(f.art_subscribed, "no metadata authority keeps iAP2 art");
    assert!(!f.position_subscribed, "playback authority drops the iAP2 bar");
    assert!(link_rx.try_recv().is_ok());
  }
}
