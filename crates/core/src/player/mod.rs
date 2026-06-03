mod state;

use libbridgething::{
  QueueItem, RepeatMode,
  client::{BridgeToClientPlayerMsg, PlayerQueueReply, PlayerStateReply},
  wire::MsgMeta,
};
pub use state::NowPlayingSource;
use state::*;
use tokio::sync::{broadcast, mpsc, watch};

use crate::{
  asset::{AssetCache, AssetCacheEvent, Retention},
  authority::AuthorityRegistry,
  net::{WSError, WireEventBus},
};

const PLAYER_CMD_CAPACITY: usize = 64;

#[derive(Debug, Clone)]
pub struct PlayerSnapshot {
  pub state_reply: PlayerStateReply,
  pub queue_reply: PlayerQueueReply,
  pub iap2_shuffle: Option<bool>,
  pub iap2_repeat_mode: Option<RepeatMode>,
  pub iap2_set_elapsed_time_available: Option<bool>,
  pub current_artwork_id: Option<String>,
}

#[derive(Debug)]
enum PlayerCommand {
  SendState,
  ApplyNowPlaying(NowPlayingSource, libbridgething::NowPlayingUpdate),
  ApplyArtworkId(NowPlayingSource, String),
  ApplyIap2Queue(Vec<QueueItem>),
  ApplyCompanionQueue(libbridgething::gateway::QueueSnapshot),
  ApplyCompanionSnapshot(libbridgething::PlayerState),
  ApplyEnrichment(libbridgething::gateway::NowPlayingEnrichment),
  ApplyTransportIntent(bool),
  ApplySeekIntent(u32),
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

  pub async fn apply_now_playing(
    &self,
    source: NowPlayingSource,
    update: libbridgething::NowPlayingUpdate,
  ) -> PlayerResult<()> {
    self.send(PlayerCommand::ApplyNowPlaying(source, update)).await
  }

  pub async fn apply_artwork_id(&self, source: NowPlayingSource, asset_id: String) -> PlayerResult<()> {
    self.send(PlayerCommand::ApplyArtworkId(source, asset_id)).await
  }

  pub async fn apply_iap2_queue(&self, items: Vec<QueueItem>) -> PlayerResult<()> {
    self.send(PlayerCommand::ApplyIap2Queue(items)).await
  }

  pub async fn apply_companion_queue(&self, snapshot: libbridgething::gateway::QueueSnapshot) -> PlayerResult<()> {
    self.send(PlayerCommand::ApplyCompanionQueue(snapshot)).await
  }

  pub async fn apply_companion_snapshot(&self, snapshot: libbridgething::PlayerState) -> PlayerResult<()> {
    self.send(PlayerCommand::ApplyCompanionSnapshot(snapshot)).await
  }

  pub async fn apply_enrichment(&self, offer: libbridgething::gateway::NowPlayingEnrichment) -> PlayerResult<()> {
    self.send(PlayerCommand::ApplyEnrichment(offer)).await
  }

  pub async fn apply_transport_intent(&self, playing: bool) -> PlayerResult<()> {
    self.send(PlayerCommand::ApplyTransportIntent(playing)).await
  }

  pub async fn apply_seek_intent(&self, position_ms: u32) -> PlayerResult<()> {
    self.send(PlayerCommand::ApplySeekIntent(position_ms)).await
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
    current_artwork_id: state.current_artwork_id(),
  }
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
  let mut asset_events = Some(assets.subscribe());

  loop {
    let force;
    let kind;
    tokio::select! {
      cmd = cmd_rx.recv() => {
        let Some(cmd) = cmd else {
          tracing::debug!("player actor: command channel closed; exiting");
          return;
        };
        kind = ProcessedKind::for_command(&cmd);
        force = forces_broadcast(&cmd);
        match cmd {
          PlayerCommand::SendState => {}
          PlayerCommand::ApplyNowPlaying(source, update) => state.apply_now_playing(source, update),
          PlayerCommand::ApplyArtworkId(source, id) => state.apply_artwork_id(source, id),
          PlayerCommand::ApplyIap2Queue(items) => state.replace_iap2_queue(items),
          PlayerCommand::ApplyCompanionQueue(snapshot) => state.apply_companion_queue(snapshot),
          PlayerCommand::ApplyCompanionSnapshot(snap) => state.apply_companion_snapshot(snap),
          PlayerCommand::ApplyEnrichment(offer) => state.apply_enrichment(offer),
          PlayerCommand::ApplyTransportIntent(playing) => state.set_transport_intent(playing),
          PlayerCommand::ApplySeekIntent(position_ms) => state.set_seek_intent(position_ms),
        }
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

    let snapshot = snapshot_of(&state);
    let _ = snapshot_tx.send(snapshot.clone());

    let sig = BroadcastSig::of(&snapshot);
    if force || last_sig.as_ref() != Some(&sig) {
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
    let _ = assets.set_retention(&old, Retention::MEM_LRU).await;
  }
  if let Some(new) = desired {
    let _ = assets.set_retention(&new, Retention::MEM_PINNED).await;
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

fn forces_broadcast(cmd: &PlayerCommand) -> bool {
  matches!(
    cmd,
    PlayerCommand::SendState
      | PlayerCommand::ApplyNowPlaying(..)
      | PlayerCommand::ApplyCompanionSnapshot(..)
      | PlayerCommand::ApplyTransportIntent(..)
      | PlayerCommand::ApplySeekIntent(..)
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
      PlayerCommand::ApplyIap2Queue(_) | PlayerCommand::ApplyCompanionQueue(_) => Self::QueueOnly,
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
