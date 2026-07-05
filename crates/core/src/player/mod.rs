mod state;

use libbridgething::{
  QueueItem, RepeatMode,
  client::{BridgeToClientPlayerMsg, PlayerQueueReply, PlayerStateReply},
  wire::MsgMeta,
};
use state::*;
use tokio::sync::{broadcast, mpsc, watch};

use crate::{
  asset::{AssetCache, AssetCacheEvent, Retention},
  authority::AuthorityRegistry,
  net::{WSError, WireEventBus},
};

const PLAYER_CMD_CAPACITY: usize = 64;

pub(crate) fn is_synthetic_uri(uri: &str) -> bool {
  uri.starts_with("iap2:")
}

#[derive(Debug, Clone)]
pub struct PlayerSnapshot {
  pub state_reply: PlayerStateReply,
  pub queue_reply: PlayerQueueReply,
  pub iap2_shuffle: Option<bool>,
  pub iap2_repeat_mode: Option<RepeatMode>,
  pub iap2_set_elapsed_time_available: Option<bool>,
  pub iap2_app_bundle: Option<String>,
  pub iap2_playing: Option<bool>,
  pub current_artwork_id: Option<String>,
  pub root_browse_gen: u64,
  pub home_recents: Vec<QueueItem>,
  pub companion_playback_authoritative: bool,
}

#[derive(Debug)]
enum PlayerCommand {
  SendState,
  ApplyNowPlaying(libbridgething::NowPlayingUpdate),
  ApplyArtworkId(String),
  ApplyCompanionQueue(libbridgething::gateway::QueueSnapshot),
  ApplyCompanionSnapshot(libbridgething::PlayerState),
  ApplyTransportIntent(bool),
  ApplySeekIntent(u32),
  ResetCompanion,
  NoteLibraryChanged,
}

#[derive(Debug, Clone)]
pub struct Player {
  cmd_tx: mpsc::Sender<PlayerCommand>,
  snapshot_rx: watch::Receiver<PlayerSnapshot>,
}

impl Player {
  pub fn new(bus: WireEventBus, authority: AuthorityRegistry, assets: AssetCache) -> Self {
    let (cmd_tx, cmd_rx) = mpsc::channel(PLAYER_CMD_CAPACITY);
    let initial = PlayerState::new(authority);
    let (snapshot_tx, snapshot_rx) = watch::channel(snapshot_of(&initial));
    tokio::spawn(run_actor(initial, cmd_rx, snapshot_tx, bus, assets));
    Self { cmd_tx, snapshot_rx }
  }

  pub async fn send_state(&self) -> PlayerResult<()> {
    self.send(PlayerCommand::SendState).await
  }

  pub async fn apply_now_playing(&self, update: libbridgething::NowPlayingUpdate) -> PlayerResult<()> {
    self.send(PlayerCommand::ApplyNowPlaying(update)).await
  }

  pub async fn apply_artwork_id(&self, asset_id: String) -> PlayerResult<()> {
    self.send(PlayerCommand::ApplyArtworkId(asset_id)).await
  }

  pub async fn apply_companion_queue(&self, snapshot: libbridgething::gateway::QueueSnapshot) -> PlayerResult<()> {
    self.send(PlayerCommand::ApplyCompanionQueue(snapshot)).await
  }

  pub async fn apply_companion_snapshot(&self, snapshot: libbridgething::PlayerState) -> PlayerResult<()> {
    self.send(PlayerCommand::ApplyCompanionSnapshot(snapshot)).await
  }

  pub async fn apply_transport_intent(&self, playing: bool) -> PlayerResult<()> {
    self.send(PlayerCommand::ApplyTransportIntent(playing)).await
  }

  pub async fn apply_seek_intent(&self, position_ms: u32) -> PlayerResult<()> {
    self.send(PlayerCommand::ApplySeekIntent(position_ms)).await
  }

  pub async fn reset_companion(&self) -> PlayerResult<()> {
    self.send(PlayerCommand::ResetCompanion).await
  }

  pub async fn note_library_changed(&self) -> PlayerResult<()> {
    self.send(PlayerCommand::NoteLibraryChanged).await
  }

  pub fn root_browse_gen(&self) -> u64 {
    self.snapshot_rx.borrow().root_browse_gen
  }

  pub fn home_recents(&self) -> Vec<QueueItem> {
    self.snapshot_rx.borrow().home_recents.clone()
  }

  pub fn state_reply(&self) -> PlayerStateReply {
    self.snapshot_rx.borrow().state_reply.clone()
  }

  pub fn queue_reply(&self) -> PlayerQueueReply {
    self.snapshot_rx.borrow().queue_reply.clone()
  }

  pub fn current_artwork_id(&self) -> Option<String> {
    self.snapshot_rx.borrow().current_artwork_id.clone()
  }

  pub fn iap2_shuffle(&self) -> Option<bool> {
    self.snapshot_rx.borrow().iap2_shuffle
  }

  pub fn iap2_repeat_mode(&self) -> Option<RepeatMode> {
    self.snapshot_rx.borrow().iap2_repeat_mode
  }

  pub fn iap2_set_elapsed_time_available(&self) -> Option<bool> {
    self.snapshot_rx.borrow().iap2_set_elapsed_time_available
  }

  pub fn iap2_app_bundle(&self) -> Option<String> {
    self.snapshot_rx.borrow().iap2_app_bundle.clone()
  }

  pub fn iap2_playing(&self) -> Option<bool> {
    self.snapshot_rx.borrow().iap2_playing
  }

  pub fn companion_playback_authoritative(&self) -> bool {
    self.snapshot_rx.borrow().companion_playback_authoritative
  }

  async fn send(&self, cmd: PlayerCommand) -> PlayerResult<()> {
    self.cmd_tx.send(cmd).await.map_err(|_| PlayerError::ActorDropped)
  }

  #[cfg(feature = "test-tap")]
  pub fn snapshot_watch(&self) -> watch::Receiver<PlayerSnapshot> {
    self.snapshot_rx.clone()
  }
}

fn snapshot_of(state: &PlayerState) -> PlayerSnapshot {
  let (state_reply, queue_reply) = state.replies();
  PlayerSnapshot {
    state_reply,
    queue_reply,
    iap2_shuffle: state.iap2_shuffle(),
    iap2_repeat_mode: state.iap2_repeat_mode(),
    iap2_set_elapsed_time_available: state.iap2_set_elapsed_time_available(),
    iap2_app_bundle: state.iap2_app_bundle(),
    iap2_playing: state.iap2_playing(),
    current_artwork_id: state.current_artwork_id(),
    root_browse_gen: state.root_browse_gen(),
    home_recents: state.home_recents(),
    companion_playback_authoritative: state.companion_playback_authoritative(),
  }
}

fn rolled_off(prev: &Option<QueueItem>, current: &Option<QueueItem>) -> Option<QueueItem> {
  let prev = prev.as_ref()?;
  let prev_pid = prev.persistent_id.as_deref()?;
  if prev.title.as_deref().is_none_or(|t| t.trim().is_empty()) {
    return None;
  }
  let current_pid = current.as_ref().and_then(|q| q.persistent_id.as_deref());
  (current_pid != Some(prev_pid)).then(|| prev.clone())
}

async fn run_actor(
  mut state: PlayerState,
  mut cmd_rx: mpsc::Receiver<PlayerCommand>,
  snapshot_tx: watch::Sender<PlayerSnapshot>,
  bus: WireEventBus,
  assets: AssetCache,
) {
  let mut last_sig: Option<BroadcastSig> = None;
  let mut pinned_head: Option<String> = None;
  let mut prev_current: Option<QueueItem> = None;
  let mut asset_events = Some(assets.subscribe());

  loop {
    let force;
    let kind;
    let mut outgoing_position_ms = 0;
    tokio::select! {
      cmd = cmd_rx.recv() => {
        let Some(cmd) = cmd else {
          tracing::debug!("player actor: command channel closed; exiting");
          return;
        };
        kind = ProcessedKind::for_command(&cmd);
        let cmd_force = forces_broadcast(&cmd);
        outgoing_position_ms = state.current_position_ms();
        match cmd {
          PlayerCommand::SendState => {}
          PlayerCommand::ApplyNowPlaying(update) => state.apply_now_playing(update),
          PlayerCommand::ApplyArtworkId(id) => state.apply_artwork_id(id),
          PlayerCommand::ApplyCompanionQueue(snapshot) => state.apply_companion_queue(snapshot),
          PlayerCommand::ApplyCompanionSnapshot(snap) => state.apply_companion_snapshot(snap),
          PlayerCommand::ApplyTransportIntent(playing) => state.set_transport_intent(playing),
          PlayerCommand::ApplySeekIntent(position_ms) => state.set_seek_intent(position_ms),
          PlayerCommand::ResetCompanion => state.reset_companion(),
          PlayerCommand::NoteLibraryChanged => state.note_library_changed(),
        }
        force = cmd_force || state.take_position_resync();
      }
      ev = async {
        match &mut asset_events {
          Some(rx) => rx.recv().await,
          None => std::future::pending().await,
        }
      } => {
        kind = ProcessedKind::Full;
        force = false;
        match ev {
          Ok(AssetCacheEvent::Ready { id }) => state.note_asset_ready(id),
          Ok(AssetCacheEvent::Cleared { id }) => state.note_asset_cleared(&id),
          Err(broadcast::error::RecvError::Lagged(n)) => {
            tracing::warn!(skipped = n, "player actor: asset event channel lagged; presence mirror may drift until next event");
          }
          Err(broadcast::error::RecvError::Closed) => {
            asset_events = None;
            continue;
          }
        }
      }
    }

    let mut snapshot = snapshot_of(&state);
    if let Some(outgoing) = rolled_off(&prev_current, &snapshot.queue_reply.current) {
      state.note_rolled_off(outgoing, outgoing_position_ms);
      snapshot = snapshot_of(&state);
    }
    prev_current = snapshot.queue_reply.current.clone();
    let _ = snapshot_tx.send(snapshot.clone());

    let sig = BroadcastSig::of(&snapshot);
    if !holds_incomplete_transition(&snapshot, last_sig.as_ref()) && (force || last_sig.as_ref() != Some(&sig)) {
      last_sig = Some(sig);
      if let Err(err) = broadcast_for(&bus, kind, snapshot).await {
        tracing::warn!(?err, "player actor: snapshot broadcast failed");
      }
    }

    reconcile_head_pin(&assets, &state, &mut pinned_head).await;
  }
}

async fn reconcile_head_pin(assets: &AssetCache, state: &PlayerState, pinned_head: &mut Option<String>) {
  let desired = state.current_artwork_id().filter(|id| state.is_present(id));
  if desired.as_deref() == pinned_head.as_deref() {
    return;
  }
  if let Some(old) = pinned_head.take() {
    let _ = assets.set_retention(&old, Retention::DISK_LRU).await;
  }
  if let Some(new) = desired {
    let _ = assets.set_retention(&new, Retention::DISK_PINNED).await;
    *pinned_head = Some(new);
  }
}

#[derive(PartialEq)]
struct BroadcastSig {
  state: PlayerStateReply,
  queue: PlayerQueueReply,
}

impl BroadcastSig {
  fn of(snapshot: &PlayerSnapshot) -> Self {
    let mut state = snapshot.state_reply.clone();
    state.state.playback.position_ms = 0;
    BroadcastSig {
      state,
      queue: snapshot.queue_reply.clone(),
    }
  }
}

fn holds_incomplete_transition(snapshot: &PlayerSnapshot, last: Option<&BroadcastSig>) -> bool {
  let Some(new) = snapshot.state_reply.state.track.as_ref() else {
    return false;
  };
  if new.title.as_deref().is_some_and(|t| !t.trim().is_empty()) {
    return false;
  }
  let Some(prev) = last.and_then(|s| s.state.state.track.as_ref()) else {
    return false;
  };
  let prev_titled = prev.title.as_deref().is_some_and(|t| !t.trim().is_empty());
  prev_titled && new.persistent_id != prev.persistent_id
}

fn forces_broadcast(cmd: &PlayerCommand) -> bool {
  matches!(
    cmd,
    PlayerCommand::SendState
      | PlayerCommand::ApplyTransportIntent(..)
      | PlayerCommand::ApplySeekIntent(..)
      | PlayerCommand::ResetCompanion
  )
}

#[derive(Debug, Clone, Copy)]
enum ProcessedKind {
  Full,
  QueueOnly,
}

impl ProcessedKind {
  fn for_command(cmd: &PlayerCommand) -> Self {
    match cmd {
      PlayerCommand::ApplyCompanionQueue(_) => Self::QueueOnly,
      _ => Self::Full,
    }
  }
}

async fn broadcast_for(bus: &WireEventBus, kind: ProcessedKind, snapshot: PlayerSnapshot) -> PlayerResult<()> {
  match kind {
    ProcessedKind::Full => {
      bus
        .broadcast(BridgeToClientPlayerMsg::Snapshot(snapshot.state_reply), MsgMeta::Event)
        .await?;
      bus
        .broadcast(
          BridgeToClientPlayerMsg::QueueChanged(snapshot.queue_reply),
          MsgMeta::Event,
        )
        .await?;
    }
    ProcessedKind::QueueOnly => {
      bus
        .broadcast(
          BridgeToClientPlayerMsg::QueueChanged(snapshot.queue_reply),
          MsgMeta::Event,
        )
        .await?;
    }
  }
  Ok(())
}

pub type PlayerResult<T> = Result<T, PlayerError>;
#[derive(Debug, thiserror::Error)]
pub enum PlayerError {
  #[error(transparent)]
  WS(#[from] WSError),
  #[error("player actor task has exited")]
  ActorDropped,
}

crate::impl_broadcast_failure_from!(PlayerError);

#[cfg(test)]
mod tests {
  use libbridgething::{MediaItemUpdate, NowPlayingUpdate};

  use super::*;
  use crate::authority::AuthorityRegistry;

  fn titled(pid: &str, title: &str) -> NowPlayingUpdate {
    NowPlayingUpdate {
      media_item: Some(MediaItemUpdate {
        persistent_id: Some(pid.into()),
        title: Some(title.into()),
        duration_ms: Some(180_000),
        ..MediaItemUpdate::default()
      }),
      playback: None,
    }
  }

  fn pid_only(pid: &str) -> NowPlayingUpdate {
    NowPlayingUpdate {
      media_item: Some(MediaItemUpdate {
        persistent_id: Some(pid.into()),
        ..MediaItemUpdate::default()
      }),
      playback: None,
    }
  }

  fn snap_after(updates: &[NowPlayingUpdate]) -> PlayerSnapshot {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    for u in updates {
      state.apply_now_playing(u.clone());
    }
    snapshot_of(&state)
  }

  #[test]
  fn holds_title_less_real_track_change() {
    let last = BroadcastSig::of(&snap_after(&[titled("iap2:track:a", "Song A")]));
    let new = snap_after(&[titled("iap2:track:a", "Song A"), pid_only("iap2:track:b")]);
    assert!(holds_incomplete_transition(&new, Some(&last)));
  }

  #[test]
  fn releases_once_new_track_has_title() {
    let last = BroadcastSig::of(&snap_after(&[titled("iap2:track:a", "Song A")]));
    let new = snap_after(&[titled("iap2:track:a", "Song A"), titled("iap2:track:b", "Song B")]);
    assert!(!holds_incomplete_transition(&new, Some(&last)));
  }

  #[test]
  fn does_not_hold_cold_start() {
    let new = snap_after(&[pid_only("iap2:track:b")]);
    assert!(!holds_incomplete_transition(&new, None));
  }

  #[test]
  fn companion_snapshot_rides_the_signature_gate() {
    // a companion snapshot must not bypass the broadcast gate: identical re-sends stay silent
    // and real changes broadcast via the signature diff or the position-resync flag.
    assert!(!forces_broadcast(&PlayerCommand::ApplyCompanionSnapshot(
      libbridgething::PlayerState::default()
    )));
  }

  #[test]
  fn idle_unlatches_the_hold() {
    let last = BroadcastSig::of(&snap_after(&[titled("iap2:track:a", "Song A")]));
    let idle = NowPlayingUpdate {
      media_item: Some(MediaItemUpdate {
        persistent_id: Some("iap2:track:0000000000000000".into()),
        title: Some(String::new()),
        ..MediaItemUpdate::default()
      }),
      playback: None,
    };
    let new = snap_after(&[titled("iap2:track:a", "Song A"), idle]);
    assert!(!holds_incomplete_transition(&new, Some(&last)));
  }
}
