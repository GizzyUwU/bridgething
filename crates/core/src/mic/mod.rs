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
#[cfg(feature = "mic")]
mod wakeword;

#[derive(Debug, Clone, Copy, serde::Deserialize)]
struct Detection {
  score: f32,
}

async fn next_detection(hits: &mut Option<mpsc::Receiver<Detection>>) -> Detection {
  let Some(hits) = hits else {
    return std::future::pending().await;
  };
  match hits.recv().await {
    Some(hit) => hit,
    None => std::future::pending().await,
  }
}

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
      frame_samples: 256,
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
  #[cfg(feature = "mic")]
  pub dsp: bridgething_dsp::pipeline::Config,
  #[cfg(feature = "mic")]
  pub wakeword_socket: std::path::PathBuf,
}

impl Default for MicConfig {
  fn default() -> Self {
    Self {
      format: CaptureFormat::default(),
      device: "hw:0,0".to_string(),
      #[cfg(feature = "mic")]
      dsp: bridgething_dsp::pipeline::Config {
        adaptation: Some(bridgething_dsp::scene::Config::default()),
        ..bridgething_dsp::pipeline::Config::default()
      },
      #[cfg(feature = "mic")]
      wakeword_socket: std::path::PathBuf::from("/run/bridgething-wakeword.sock"),
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
  let (frame_tx, mut frame_rx) = mpsc::channel::<bytes::Bytes>(64);
  #[cfg(feature = "mic")]
  let (link, hits, _link_handle) = wakeword::WakeWordLink::spawn(config.wakeword_socket.clone());
  #[cfg(feature = "mic")]
  let mut hits = Some(hits);
  #[cfg(not(feature = "mic"))]
  let mut hits = None;
  #[cfg(feature = "mic")]
  let mut capture = open_capture(&config, &frame_tx);
  #[cfg(not(feature = "mic"))]
  let mut capture: Option<Capture> = None;
  let mut stream: Option<Stream> = None;

  loop {
    tokio::select! {
      cmd = rx.recv() => {
        let Some(cmd) = cmd else { break; };
        handle_cmd(cmd, &state, &bus, &bluetooth, &config, &frame_tx, &mut capture, &mut stream).await;
      }
      Some(frame) = frame_rx.recv() => {
        #[cfg(feature = "mic")]
        link.offer(frame.clone());
        if let Some(open) = stream.as_mut() {
          forward_frame(frame, open, &bluetooth).await;
        }
      }
      hit = next_detection(&mut hits) => {
        on_wake_word(hit, &state, &bus, &bluetooth, &config, &frame_tx, &mut capture, &mut stream).await;
      }
      else => break,
    }
  }

  if let Some(c) = capture {
    c.stop();
  }
  tracing::debug!("mic manager loop exiting");
}

fn open_capture(config: &MicConfig, frames: &mpsc::Sender<bytes::Bytes>) -> Option<Capture> {
  match Capture::start(config.clone(), frames.clone()) {
    Ok(capture) => {
      tracing::info!(device = config.device, "microphone open");
      Some(capture)
    }
    Err(err) => {
      tracing::warn!("microphone unavailable: {err}");
      None
    }
  }
}

async fn on_wake_word(
  hit: Detection,
  state: &Arc<RwLock<State>>,
  bus: &WireEventBus,
  bluetooth: &BluetoothMan,
  config: &MicConfig,
  frames: &mpsc::Sender<bytes::Bytes>,
  capture: &mut Option<Capture>,
  stream: &mut Option<Stream>,
) {
  if let Some(open) = capture.as_ref() {
    open.mark_target();
  }
  if state.read().await.muted {
    tracing::debug!("wake word fired while muted; ignoring");
    return;
  }
  match start_stream(state, bus, bluetooth, config, frames, capture, stream).await {
    Ok(id) => tracing::info!(score = hit.score, stream = %id, "wake word opened the uplink"),
    Err(err) => tracing::warn!("wake word could not open the uplink: {err}"),
  }
}

async fn handle_cmd(
  cmd: Cmd,
  state: &Arc<RwLock<State>>,
  bus: &WireEventBus,
  bluetooth: &BluetoothMan,
  config: &MicConfig,
  frame_tx: &mpsc::Sender<bytes::Bytes>,
  capture: &mut Option<Capture>,
  stream: &mut Option<Stream>,
) {
  match cmd {
    Cmd::Start { reply } => {
      let outcome = start_stream(state, bus, bluetooth, config, frame_tx, capture, stream).await;
      let _ = reply.send(outcome);
    }
    Cmd::Stop { reason, reply } => {
      stop_stream(state, bus, bluetooth, stream, reason).await;
      let _ = reply.send(());
    }
    Cmd::Mute { preserve, reply } => {
      let was_open = {
        let mut guard = state.write().await;
        guard.muted = true;
        guard.capturing
      };
      if let Some(open) = capture.take() {
        open.stop();
      }
      if was_open && !preserve {
        stop_stream(state, bus, bluetooth, stream, VoiceCloseReason::Muted).await;
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
      if capture.is_none() {
        *capture = open_capture(config, frame_tx);
      }
      broadcast_state(bus, state).await;
      let _ = reply.send(());
    }
  }
}

async fn start_stream(
  state: &Arc<RwLock<State>>,
  bus: &WireEventBus,
  bluetooth: &BluetoothMan,
  config: &MicConfig,
  frame_tx: &mpsc::Sender<bytes::Bytes>,
  capture: &mut Option<Capture>,
  stream: &mut Option<Stream>,
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

  if capture.is_none() {
    *capture = open_capture(config, frame_tx);
  }
  if capture.is_none() {
    return Err(MicError::Unavailable);
  }

  let stream_id = Uuid::now_v7();
  *stream = Some(Stream { id: stream_id, seq: 0 });

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

async fn stop_stream(
  state: &Arc<RwLock<State>>,
  bus: &WireEventBus,
  bluetooth: &BluetoothMan,
  stream: &mut Option<Stream>,
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
  stream.take();
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

async fn forward_frame(pcm: bytes::Bytes, stream: &mut Stream, bluetooth: &BluetoothMan) {
  let seq = stream.seq;
  stream.seq = stream.seq.wrapping_add(1);
  bluetooth
    .gateway_man
    .broadcast_event_bulk(BridgeToGatewayVoiceMsgEvent::Frame(VoiceFrame {
      stream_id: stream.id,
      seq,
      pcm,
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

#[derive(Debug)]
struct Capture {
  #[cfg(feature = "mic")]
  worker: alsa_capture::WorkerHandle,
}

impl Capture {
  #[cfg(feature = "mic")]
  fn start(config: MicConfig, frames: mpsc::Sender<bytes::Bytes>) -> Result<Self, MicError> {
    Ok(Self {
      worker: alsa_capture::WorkerHandle::start(config, frames)?,
    })
  }

  #[cfg(not(feature = "mic"))]
  fn start(_config: MicConfig, _frames: mpsc::Sender<bytes::Bytes>) -> Result<Self, MicError> {
    Err(MicError::Unavailable)
  }

  #[cfg(feature = "mic")]
  fn mark_target(&self) {
    self.worker.mark_target();
  }

  #[cfg(not(feature = "mic"))]
  fn mark_target(&self) {}

  fn stop(self) {
    #[cfg(feature = "mic")]
    self.worker.stop();
  }
}

struct Stream {
  id: Uuid,
  seq: u32,
}
