mod state;

use libbridgething::{
  QueueItem, RepeatMode,
  client::{BridgeToClientPlayerMsg, PlayerQueueReply, PlayerStateReply},
  wire::MsgMeta,
};
pub use state::NowPlayingSource;
use state::*;
use tokio::sync::{mpsc, watch};

use crate::{
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
  ApplyCompanionQueue(Vec<QueueItem>),
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
  pub fn new(bus: WireEventBus, authority: AuthorityRegistry) -> Self {
    let (cmd_tx, cmd_rx) = mpsc::channel(PLAYER_CMD_CAPACITY);
    let initial = PlayerState::new(authority);
    let (snapshot_tx, snapshot_rx) = watch::channel(snapshot_of(&initial));
    tokio::spawn(run_actor(initial, cmd_rx, snapshot_tx, bus));
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

  pub async fn apply_companion_queue(&self, items: Vec<QueueItem>) -> PlayerResult<()> {
    self.send(PlayerCommand::ApplyCompanionQueue(items)).await
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
) {
  let mut last_sig: Option<BroadcastSig> = None;
  while let Some(cmd) = cmd_rx.recv().await {
    let kind = ProcessedKind::for_command(&cmd);
    let force = forces_broadcast(&cmd);
    match cmd {
      PlayerCommand::SendState => {}
      PlayerCommand::ApplyNowPlaying(source, update) => state.apply_now_playing(source, update),
      PlayerCommand::ApplyArtworkId(source, id) => state.apply_artwork_id(source, id),
      PlayerCommand::ApplyIap2Queue(items) => state.replace_iap2_queue(items),
      PlayerCommand::ApplyCompanionQueue(items) => state.replace_companion_queue(items),
      PlayerCommand::ApplyCompanionSnapshot(snap) => state.apply_companion_snapshot(snap),
      PlayerCommand::ApplyEnrichment(offer) => state.apply_enrichment(offer),
      PlayerCommand::ApplyTransportIntent(playing) => state.set_transport_intent(playing),
      PlayerCommand::ApplySeekIntent(position_ms) => state.set_seek_intent(position_ms),
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
  }
  tracing::debug!("player actor: command channel closed; exiting");
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
