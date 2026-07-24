use std::sync::Arc;

use libbridgething::{
  client::{BridgeToClientVoiceMsg, VoiceState},
  gateway::{
    BridgeToGatewayVoiceMsgEvent, VoiceCloseReason, VoiceFormat, VoiceFrame, VoiceStreamClose, VoiceStreamOpen,
  },
  wire::MsgMeta,
};
use tokio::{
  sync::{RwLock, mpsc, oneshot},
  task::JoinHandle,
};
use uuid::Uuid;

use crate::{bluetooth::BluetoothMan, net::WireEventBus};

#[cfg(feature = "mic")]
mod alsa_capture;

#[derive(Debug, Clone, Copy)]
pub struct CaptureFormat {
  pub sample_rate_hz: u32,
  pub channels: u16,
  pub bits_per_sample: u16,
  pub frame_samples: u32,
}

impl Default for CaptureFormat {
  fn default() -> Self {
    Self {
      sample_rate_hz: 16_000,
      channels: 1,
      bits_per_sample: 16,
      frame_samples: 320,
    }
  }
}

impl CaptureFormat {
  pub fn wire(&self) -> VoiceFormat {
    VoiceFormat {
      sample_rate_hz: self.sample_rate_hz,
      channels: self.channels,
      bits_per_sample: self.bits_per_sample,
    }
  }
}

#[derive(Debug, Clone)]
pub struct MicConfig {
  pub format: CaptureFormat,
  pub device: String,
}

impl Default for MicConfig {
  fn default() -> Self {
    Self {
      format: CaptureFormat::default(),
      device: "default".to_string(),
    }
  }
}

#[derive(Debug, thiserror::Error)]
pub enum MicError {
  #[error("manager loop has exited")]
  Closed,
  #[error("mic capture is not available in this build")]
  Unavailable,
  #[error("alsa: {0}")]
  Alsa(String),
}

#[derive(Debug, Default)]
struct State {
  muted: bool,
  capturing: bool,
  current_stream: Option<Uuid>,
}

impl State {
  fn snapshot(&self) -> VoiceState {
    VoiceState {
      muted: self.muted,
      capturing: self.capturing,
    }
  }
}

#[derive(Debug)]
enum Cmd {
  Start {
    reply: oneshot::Sender<Result<Uuid, MicError>>,
  },
  Stop {
    reason: VoiceCloseReason,
    reply: oneshot::Sender<()>,
  },
  Mute {
    preserve: bool,
    reply: oneshot::Sender<()>,
  },
  Unmute {
    reply: oneshot::Sender<()>,
  },
}

#[derive(Debug, Clone)]
pub struct MicManager {
  state: Arc<RwLock<State>>,
  tx: mpsc::Sender<Cmd>,
}

impl MicManager {
  pub async fn init(bus: WireEventBus, bluetooth: BluetoothMan, config: MicConfig) -> MicManagerInit {
    let state = Arc::new(RwLock::new(State::default()));
    let (tx, rx) = mpsc::channel(16);
    let manager = Self {
      state: state.clone(),
      tx,
    };
    MicManagerInit {
      manager,
      rx,
      state,
      bus,
      bluetooth,
      config,
    }
  }

  pub async fn snapshot(&self) -> VoiceState {
    self.state.read().await.snapshot()
  }

  pub async fn push_to_talk(&self) -> Result<Uuid, MicError> {
    let (reply_tx, reply_rx) = oneshot::channel();
    self
      .tx
      .send(Cmd::Start { reply: reply_tx })
      .await
      .map_err(|_| MicError::Closed)?;
    reply_rx.await.map_err(|_| MicError::Closed)?
  }

  pub async fn cancel(&self) -> Result<(), MicError> {
    self.stop_with(VoiceCloseReason::Cancelled).await
  }

  pub async fn stop_with(&self, reason: VoiceCloseReason) -> Result<(), MicError> {
    let (reply_tx, reply_rx) = oneshot::channel();
    self
      .tx
      .send(Cmd::Stop {
        reason,
        reply: reply_tx,
      })
      .await
      .map_err(|_| MicError::Closed)?;
    reply_rx.await.map_err(|_| MicError::Closed)
  }

  pub async fn set_muted(&self, muted: bool, preserve: bool) -> Result<(), MicError> {
    let (reply_tx, reply_rx) = oneshot::channel();
    let cmd = if muted {
      Cmd::Mute {
        preserve,
        reply: reply_tx,
      }
    } else {
      Cmd::Unmute { reply: reply_tx }
    };
    self.tx.send(cmd).await.map_err(|_| MicError::Closed)?;
    reply_rx.await.map_err(|_| MicError::Closed)
  }
}

pub struct MicManagerInit {
  pub manager: MicManager,
  rx: mpsc::Receiver<Cmd>,
  state: Arc<RwLock<State>>,
  bus: WireEventBus,
  bluetooth: BluetoothMan,
  config: MicConfig,
}

impl MicManagerInit {
  pub fn spawn(self) -> (MicManager, JoinHandle<()>) {
    let manager = self.manager.clone();
    let handle = tokio::spawn(run_loop(self.rx, self.state, self.bus, self.bluetooth, self.config));
    (manager, handle)
  }
}

async fn run_loop(
  mut rx: mpsc::Receiver<Cmd>,
  state: Arc<RwLock<State>>,
  bus: WireEventBus,
  bluetooth: BluetoothMan,
  config: MicConfig,
) {
  let (frame_tx, mut frame_rx) = mpsc::channel::<CapturedFrame>(64);
  let mut session: Option<Session> = None;

  loop {
    tokio::select! {
      cmd = rx.recv() => {
        let Some(cmd) = cmd else { break; };
        handle_cmd(cmd, &state, &bus, &bluetooth, &config, &frame_tx, &mut session).await;
      }
      Some(frame) = frame_rx.recv() => {
        forward_frame(frame, &bluetooth, &session).await;
      }
      else => break,
    }
  }

  if let Some(s) = session {
    s.stop();
  }
  tracing::debug!("mic manager loop exiting");
}

async fn handle_cmd(
  cmd: Cmd,
  state: &Arc<RwLock<State>>,
  bus: &WireEventBus,
  bluetooth: &BluetoothMan,
  config: &MicConfig,
  frame_tx: &mpsc::Sender<CapturedFrame>,
  session: &mut Option<Session>,
) {
  match cmd {
    Cmd::Start { reply } => {
      let outcome = start_session(state, bus, bluetooth, config, frame_tx, session).await;
      let _ = reply.send(outcome);
    }
    Cmd::Stop { reason, reply } => {
      stop_session(state, bus, bluetooth, session, reason).await;
      let _ = reply.send(());
    }
    Cmd::Mute { preserve, reply } => {
      let was_capturing = {
        let mut guard = state.write().await;
        guard.muted = true;
        guard.capturing
      };
      if was_capturing && !preserve {
        stop_session(state, bus, bluetooth, session, VoiceCloseReason::Muted).await;
      } else {
        broadcast_state(bus, state).await;
      }
      let _ = reply.send(());
    }
    Cmd::Unmute { reply } => {
      {
        let mut guard = state.write().await;
        guard.muted = false;
      }
      broadcast_state(bus, state).await;
      let _ = reply.send(());
    }
  }
}

async fn start_session(
  state: &Arc<RwLock<State>>,
  bus: &WireEventBus,
  bluetooth: &BluetoothMan,
  config: &MicConfig,
  frame_tx: &mpsc::Sender<CapturedFrame>,
  session: &mut Option<Session>,
) -> Result<Uuid, MicError> {
  {
    let guard = state.read().await;
    if guard.muted {
      return Err(MicError::Unavailable);
    }
    if guard.capturing
      && let Some(id) = guard.current_stream
    {
      return Ok(id);
    }
  }

  let stream_id = Uuid::now_v7();
  let started = match Session::start(stream_id, config.clone(), frame_tx.clone()) {
    Ok(s) => s,
    Err(err) => {
      tracing::warn!("mic capture start failed: {err}");
      return Err(err);
    }
  };
  *session = Some(started);

  {
    let mut guard = state.write().await;
    guard.capturing = true;
    guard.current_stream = Some(stream_id);
  }

  bluetooth
    .gateway_man
    .broadcast(BridgeToGatewayVoiceMsgEvent::StreamOpen(VoiceStreamOpen {
      stream_id,
      format: config.format.wire(),
    }))
    .await;
  broadcast_state(bus, state).await;
  Ok(stream_id)
}

async fn stop_session(
  state: &Arc<RwLock<State>>,
  bus: &WireEventBus,
  bluetooth: &BluetoothMan,
  session: &mut Option<Session>,
  reason: VoiceCloseReason,
) {
  let id = {
    let mut guard = state.write().await;
    if !guard.capturing {
      return;
    }
    guard.capturing = false;
    guard.current_stream.take()
  };
  if let Some(s) = session.take() {
    s.stop();
  }
  if let Some(stream_id) = id {
    bluetooth
      .gateway_man
      .broadcast(BridgeToGatewayVoiceMsgEvent::StreamClose(VoiceStreamClose {
        stream_id,
        reason,
      }))
      .await;
  }
  broadcast_state(bus, state).await;
}

async fn forward_frame(frame: CapturedFrame, bluetooth: &BluetoothMan, session: &Option<Session>) {
  let Some(active) = session else {
    return;
  };
  if active.stream_id != frame.stream_id {
    return;
  }
  bluetooth
    .gateway_man
    .broadcast_event_bulk(BridgeToGatewayVoiceMsgEvent::Frame(VoiceFrame {
      stream_id: frame.stream_id,
      seq: frame.seq,
      pcm: frame.pcm,
    }))
    .await;
}

async fn broadcast_state(bus: &WireEventBus, state: &Arc<RwLock<State>>) {
  let snapshot = state.read().await.snapshot();
  if let Err(errors) = bus
    .broadcast(BridgeToClientVoiceMsg::StateChanged(snapshot), MsgMeta::Event)
    .await
  {
    tracing::trace!("mic state broadcast had {} ws error(s)", errors.len());
  }
}

pub(crate) struct CapturedFrame {
  pub stream_id: Uuid,
  pub seq: u32,
  pub pcm: bytes::Bytes,
}

#[derive(Debug)]
struct Session {
  stream_id: Uuid,
  #[cfg(feature = "mic")]
  worker: alsa_capture::WorkerHandle,
}

impl Session {
  #[cfg(feature = "mic")]
  fn start(stream_id: Uuid, config: MicConfig, frames: mpsc::Sender<CapturedFrame>) -> Result<Self, MicError> {
    let worker = alsa_capture::WorkerHandle::start(stream_id, config, frames)?;
    Ok(Self { stream_id, worker })
  }

  #[cfg(not(feature = "mic"))]
  fn start(_stream_id: Uuid, _config: MicConfig, _frames: mpsc::Sender<CapturedFrame>) -> Result<Self, MicError> {
    Err(MicError::Unavailable)
  }

  fn stop(self) {
    #[cfg(feature = "mic")]
    self.worker.stop();
  }
}
