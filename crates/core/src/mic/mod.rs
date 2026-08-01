use std::sync::Arc;

use libbridgething::{
  VoiceCaptureReason, VoiceDispatchErrorCode,
  client::{BridgeToClientVoiceMsg, VoiceActivity, VoiceActivityError, VoicePhase, VoiceState},
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

const UPLINK_BACKLOG: usize = 256;

#[cfg(feature = "mic")]
type Link = Option<wakeword::WakeWordLink>;
#[cfg(not(feature = "mic"))]
type Link = ();

const RESOLVE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

#[cfg(feature = "mic")]
const WAKEWORD_THRESHOLD: f32 = 0.35;

#[derive(Debug, Clone, Copy)]
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
  pub max_uplink: std::time::Duration,
  #[cfg(feature = "mic")]
  pub dsp: bridgething_dsp::pipeline::Config,
  #[cfg(feature = "mic")]
  pub wakeword_models: Vec<std::path::PathBuf>,
  #[cfg(feature = "mic")]
  pub wakeword_threshold: f32,
}

impl Default for MicConfig {
  fn default() -> Self {
    Self {
      format: CaptureFormat::default(),
      device: "hw:0,0".to_string(),
      max_uplink: std::time::Duration::from_secs(30),
      #[cfg(feature = "mic")]
      dsp: bridgething_dsp::pipeline::Config {
        adaptation: Some(bridgething_dsp::scene::Config::default()),
        ..bridgething_dsp::pipeline::Config::default()
      },
      #[cfg(feature = "mic")]
      wakeword_models: crate::paths::wakeword_models(),
      #[cfg(feature = "mic")]
      wakeword_threshold: WAKEWORD_THRESHOLD,
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
  phase: VoicePhase,
  current_stream: Option<Uuid>,
  reason: Option<VoiceCaptureReason>,
  score: Option<f32>,
}

impl State {
  fn snapshot(&self) -> VoiceState {
    VoiceState {
      muted: self.muted,
      capturing: self.capturing,
      phase: self.phase,
    }
  }

  fn clear_turn(&mut self) {
    self.current_stream = None;
    self.reason = None;
    self.score = None;
  }

  fn activity(&self, phase: VoicePhase) -> VoiceActivity {
    VoiceActivity {
      stream_id: self.current_stream,
      reason: self.reason,
      score: self.score,
      ..VoiceActivity::new(phase)
    }
  }
}

#[derive(Debug)]
enum Cmd {
  Start {
    reason: VoiceCaptureReason,
    reply: oneshot::Sender<Result<Uuid, MicError>>,
  },
  Finish {
    activity: Box<VoiceActivity>,
    reply: oneshot::Sender<()>,
  },
  ReloadWakeWord {
    reply: oneshot::Sender<()>,
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
    self.open(VoiceCaptureReason::PushToTalk).await
  }

  pub async fn open(&self, reason: VoiceCaptureReason) -> Result<Uuid, MicError> {
    let (reply_tx, reply_rx) = oneshot::channel();
    self
      .tx
      .send(Cmd::Start {
        reason,
        reply: reply_tx,
      })
      .await
      .map_err(|_| MicError::Closed)?;
    reply_rx.await.map_err(|_| MicError::Closed)?
  }

  pub async fn reload_wakeword(&self) -> Result<(), MicError> {
    let (reply_tx, reply_rx) = oneshot::channel();
    self
      .tx
      .send(Cmd::ReloadWakeWord { reply: reply_tx })
      .await
      .map_err(|_| MicError::Closed)?;
    reply_rx.await.map_err(|_| MicError::Closed)
  }

  pub async fn finish(&self, activity: VoiceActivity) -> Result<(), MicError> {
    let (reply_tx, reply_rx) = oneshot::channel();
    self
      .tx
      .send(Cmd::Finish {
        activity: Box::new(activity),
        reply: reply_tx,
      })
      .await
      .map_err(|_| MicError::Closed)?;
    reply_rx.await.map_err(|_| MicError::Closed)
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
  let (mut link, mut hits) = match wakeword::WakeWordLink::spawn(&config.wakeword_models, config.wakeword_threshold) {
    Some((link, hits)) => (Some(link), Some(hits)),
    None => (None, None),
  };
  #[cfg(not(feature = "mic"))]
  let (mut link, mut hits): (Link, Option<mpsc::Receiver<Detection>>) = ((), None);
  #[cfg(feature = "mic")]
  let mut capture = open_capture(&config, &frame_tx);
  #[cfg(not(feature = "mic"))]
  let mut capture: Option<Capture> = None;
  let mut stream: Option<Stream> = None;

  let mut resolve_by: Option<tokio::time::Instant> = None;

  loop {
    tokio::select! {
      cmd = rx.recv() => {
        let Some(cmd) = cmd else { break; };
        handle_cmd(cmd, &state, &bus, &bluetooth, &config, &frame_tx, &mut capture, &mut stream, &mut resolve_by, &mut link, &mut hits).await;
      }
      Some(frame) = frame_rx.recv() => {
        #[cfg(feature = "mic")]
        if let Some(link) = link.as_ref() {
          link.offer(frame.clone());
        }
        if let Some(open) = stream.as_mut()
          && !open.forward(frame)
        {
          tracing::error!("gateway fell a full uplink buffer behind; ending the stream rather than holing it");
          stop_stream(&state, &bus, &bluetooth, &mut stream, VoiceCloseReason::Error, &mut resolve_by).await;
        }
      }
      hit = next_detection(&mut hits) => {
        on_wake_word(hit, &state, &bus, &bluetooth, &config, &frame_tx, &mut capture, &mut stream).await;
      }
      () = past_cap(stream.as_ref()) => {
        tracing::warn!(cap = ?config.max_uplink, "uplink hit the cap with no close from the gateway");
        stop_stream(&state, &bus, &bluetooth, &mut stream, VoiceCloseReason::Cancelled, &mut resolve_by).await;
      }
      () = past_deadline(resolve_by) => {
        tracing::warn!(wait = ?RESOLVE_TIMEOUT, "companion never answered a closed turn");
        resolve_by = None;
        let activity = {
          let mut guard = state.write().await;
          guard.phase = VoicePhase::Idle;
          let activity = VoiceActivity {
            error: Some(VoiceActivityError {
              code: VoiceDispatchErrorCode::Internal,
              msg: "companion did not answer before the resolve timeout".into(),
            }),
            ..guard.activity(VoicePhase::Failed)
          };
          guard.clear_turn();
          activity
        };
        broadcast_activity(&bus, activity).await;
        broadcast_state(&bus, &state).await;
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
  match start_stream(
    state,
    bus,
    bluetooth,
    config,
    frames,
    capture,
    stream,
    VoiceCaptureReason::WakeWord,
    Some(hit.score),
  )
  .await
  {
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
  resolve_by: &mut Option<tokio::time::Instant>,
  link: &mut Link,
  hits: &mut Option<mpsc::Receiver<Detection>>,
) {
  match cmd {
    Cmd::Start { reason, reply } => {
      let outcome = start_stream(state, bus, bluetooth, config, frame_tx, capture, stream, reason, None).await;
      let _ = reply.send(outcome);
    }
    Cmd::Stop { reason, reply } => {
      stop_stream(state, bus, bluetooth, stream, reason, resolve_by).await;
      let _ = reply.send(());
    }
    Cmd::Finish { activity, reply } => {
      *resolve_by = None;
      let filled = {
        let mut guard = state.write().await;
        guard.phase = VoicePhase::Idle;
        let filled = VoiceActivity {
          stream_id: activity.stream_id.or(guard.current_stream),
          reason: activity.reason.or(guard.reason),
          score: activity.score.or(guard.score),
          ..*activity
        };
        guard.clear_turn();
        filled
      };
      broadcast_activity(bus, filled).await;
      broadcast_state(bus, state).await;
      let _ = reply.send(());
    }
    Cmd::ReloadWakeWord { reply } => {
      #[cfg(feature = "mic")]
      {
        (*link, *hits) = match wakeword::WakeWordLink::spawn(&config.wakeword_models, config.wakeword_threshold) {
          Some((next, rx)) => (Some(next), Some(rx)),
          None => (None, None),
        };
      }
      #[cfg(not(feature = "mic"))]
      {
        let _ = (link, hits);
      }
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
        stop_stream(state, bus, bluetooth, stream, VoiceCloseReason::Muted, resolve_by).await;
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
  reason: VoiceCaptureReason,
  score: Option<f32>,
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
  *stream = Some(Stream::open(stream_id, config.max_uplink, bluetooth.clone()));

  let activity = {
    let mut guard = state.write().await;
    guard.capturing = true;
    guard.phase = VoicePhase::Listening;
    guard.current_stream = Some(stream_id);
    guard.reason = Some(reason);
    guard.score = score;
    guard.activity(VoicePhase::Listening)
  };

  bluetooth
    .gateway_man
    .broadcast(BridgeToGatewayVoiceMsgEvent::StreamOpen(VoiceStreamOpen {
      stream_id,
      format: config.format.wire(),
    }))
    .await;
  broadcast_activity(bus, activity).await;
  broadcast_state(bus, state).await;
  Ok(stream_id)
}

async fn stop_stream(
  state: &Arc<RwLock<State>>,
  bus: &WireEventBus,
  bluetooth: &BluetoothMan,
  stream: &mut Option<Stream>,
  reason: VoiceCloseReason,
  resolve_by: &mut Option<tokio::time::Instant>,
) {
  let next = match reason {
    VoiceCloseReason::EndOfSpeech => VoicePhase::Thinking,
    VoiceCloseReason::Error => VoicePhase::Failed,
    VoiceCloseReason::Cancelled | VoiceCloseReason::Muted => VoicePhase::Idle,
  };

  let (id, activity) = {
    let mut guard = state.write().await;
    if !guard.capturing {
      return;
    }
    guard.capturing = false;
    guard.phase = if next == VoicePhase::Thinking {
      VoicePhase::Thinking
    } else {
      VoicePhase::Idle
    };
    let activity = guard.activity(next);
    let id = guard.current_stream;
    if next != VoicePhase::Thinking {
      guard.clear_turn();
    }
    (id, activity)
  };

  *resolve_by = (next == VoicePhase::Thinking).then(|| tokio::time::Instant::now() + RESOLVE_TIMEOUT);
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
  broadcast_activity(bus, activity).await;
  broadcast_state(bus, state).await;
}

async fn past_cap(stream: Option<&Stream>) {
  match stream {
    Some(open) => tokio::time::sleep_until(open.deadline).await,
    None => std::future::pending().await,
  }
}

async fn past_deadline(at: Option<tokio::time::Instant>) {
  match at {
    Some(at) => tokio::time::sleep_until(at).await,
    None => std::future::pending().await,
  }
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

async fn broadcast_activity(bus: &WireEventBus, activity: VoiceActivity) {
  if let Err(errors) = bus
    .broadcast(BridgeToClientVoiceMsg::Activity(activity), MsgMeta::Event)
    .await
  {
    tracing::trace!("voice activity broadcast had {} ws error(s)", errors.len());
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
  deadline: tokio::time::Instant,
  frames: mpsc::Sender<VoiceFrame>,
  _uplink: JoinHandle<()>,
}

impl Stream {
  fn open(id: Uuid, cap: std::time::Duration, bluetooth: BluetoothMan) -> Self {
    let (frames, rx) = mpsc::channel(UPLINK_BACKLOG);
    Self {
      id,
      seq: 0,
      deadline: tokio::time::Instant::now() + cap,
      frames,
      _uplink: tokio::spawn(run_uplink(rx, bluetooth)),
    }
  }

  fn forward(&mut self, pcm: bytes::Bytes) -> bool {
    let seq = self.seq;
    self.seq = self.seq.wrapping_add(1);
    self
      .frames
      .try_send(VoiceFrame {
        stream_id: self.id,
        seq,
        pcm,
      })
      .is_ok()
  }
}

async fn run_uplink(mut frames: mpsc::Receiver<VoiceFrame>, bluetooth: BluetoothMan) {
  while let Some(frame) = frames.recv().await {
    bluetooth
      .gateway_man
      .broadcast(BridgeToGatewayVoiceMsgEvent::Frame(frame))
      .await;
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn wake_turn() -> State {
    State {
      capturing: true,
      phase: VoicePhase::Listening,
      current_stream: Some(Uuid::now_v7()),
      reason: Some(VoiceCaptureReason::WakeWord),
      score: Some(0.94),
      ..State::default()
    }
  }

  #[test]
  fn a_listening_activity_carries_the_turn_context() {
    let state = wake_turn();
    let activity = state.activity(VoicePhase::Listening);
    assert_eq!(activity.reason, Some(VoiceCaptureReason::WakeWord));
    assert_eq!(activity.score, Some(0.94));
    assert_eq!(activity.stream_id, state.current_stream);
  }

  #[test]
  fn a_settled_turn_leaves_nothing_for_the_next_one_to_inherit() {
    let mut state = wake_turn();
    state.clear_turn();
    let next = state.activity(VoicePhase::Listening);
    assert_eq!(
      next.score, None,
      "a push-to-talk turn must not report the last wake word's score"
    );
    assert_eq!(next.reason, None);
    assert_eq!(next.stream_id, None);
  }

  #[test]
  fn the_snapshot_reports_phase_alongside_mute_and_capture() {
    let state = wake_turn();
    let snapshot = state.snapshot();
    assert_eq!(snapshot.phase, VoicePhase::Listening);
    assert!(snapshot.capturing);
    assert!(!snapshot.muted);
  }
}
