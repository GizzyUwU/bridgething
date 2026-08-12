use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PhoneCallStatus {
  Disconnected,
  Sending,
  Ringing,
  Connecting,
  Active,
  Held,
  Disconnecting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PhoneCallDirection {
  Incoming,
  Outgoing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PhoneCallService {
  Unknown,
  Telephony,
  FaceTimeAudio,
  FaceTimeVideo,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PhoneCall {
  pub call_id: String,
  pub remote_id: String,
  pub display_name: String,
  pub status: PhoneCallStatus,
  pub direction: PhoneCallDirection,
  pub started_at_unix_s: Option<u32>,
  pub label: Option<String>,
  pub address_book_id: Option<String>,
  pub service: Option<PhoneCallService>,
  pub is_conferenced: Option<bool>,
  pub conference_group: Option<u8>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PhoneState {
  pub active_calls: Vec<PhoneCall>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum CallEndReason {
  Local,
  Remote,
  Missed,
  Declined,
  Failed { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PhoneCallEnded {
  pub call_id: String,
  pub reason: CallEndReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum RegistrationStatus {
  Unknown,
  NotRegistered,
  Searching,
  Denied,
  RegisteredHome,
  RegisteredRoaming,
  EmergencyCallsOnly,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CommunicationsState {
  pub signal_strength: Option<u8>,
  pub registration_status: Option<RegistrationStatus>,
  pub airplane_mode: Option<bool>,
  pub carrier_name: Option<String>,
  pub cellular_supported: Option<bool>,
  pub telephony_enabled: Option<bool>,
  pub face_time_audio_enabled: Option<bool>,
  pub face_time_video_enabled: Option<bool>,
  pub mute_status: Option<bool>,
  pub current_call_count: Option<u8>,
  pub new_voicemail_count: Option<u8>,
  pub initiate_call_available: Option<bool>,
  pub end_and_accept_available: Option<bool>,
  pub hold_and_accept_available: Option<bool>,
  pub swap_available: Option<bool>,
  pub merge_available: Option<bool>,
  pub hold_available: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum DtmfTone {
  D0,
  D1,
  D2,
  D3,
  D4,
  D5,
  D6,
  D7,
  D8,
  D9,
  Star,
  Hash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AcceptCallAction {
  Accept,
  EndAndAccept,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EndCallAction {
  End,
  EndAll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum InitiateCallType {
  Destination,
  Voicemail,
  Redial,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PhoneInitiate {
  pub kind: InitiateCallType,
  pub destination_id: Option<String>,
  pub service: Option<PhoneCallService>,
  pub address_book_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum PhoneCommand {
  Answer { call_id: String },
  Accept { call_id: String, action: AcceptCallAction },
  Decline { call_id: String },
  End { call_id: String },
  EndTyped { call_id: String, action: EndCallAction },
  Hold { call_id: String },
  Unhold { call_id: String },
  Initiate { action: PhoneInitiate },
  Swap,
  Merge,
  Mute { muted: bool },
  Dtmf { call_id: Option<String>, tone: DtmfTone },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhoneEvent {
  CallStarted(PhoneCall),
  CallUpdated(PhoneCall),
  CallEnded(PhoneCallEnded),
  State(PhoneState),
  Communications(CommunicationsState),
}

#[uniffi::export(with_foreign)]
pub trait PhoneBackend: Send + Sync {
  fn start(&self, inbox: Arc<PhoneInbox>);
  fn stop(&self);
  fn command(&self, cmd: PhoneCommand);
  fn state_get(&self, sink: Arc<PhoneStateSink>);
}

#[derive(uniffi::Object)]
pub struct PhoneInbox {
  tx: mpsc::UnboundedSender<PhoneEvent>,
}

impl PhoneInbox {
  pub fn channel() -> (Arc<Self>, mpsc::UnboundedReceiver<PhoneEvent>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (Arc::new(Self { tx }), rx)
  }
}

#[uniffi::export]
impl PhoneInbox {
  pub fn on_call_started(&self, call: PhoneCall) {
    let _ = self.tx.send(PhoneEvent::CallStarted(call));
  }

  pub fn on_call_updated(&self, call: PhoneCall) {
    let _ = self.tx.send(PhoneEvent::CallUpdated(call));
  }

  pub fn on_call_ended(&self, ended: PhoneCallEnded) {
    let _ = self.tx.send(PhoneEvent::CallEnded(ended));
  }

  pub fn on_state(&self, state: PhoneState) {
    let _ = self.tx.send(PhoneEvent::State(state));
  }

  pub fn on_communications(&self, state: CommunicationsState) {
    let _ = self.tx.send(PhoneEvent::Communications(state));
  }
}

#[derive(uniffi::Object)]
pub struct PhoneStateSink {
  tx: std::sync::Mutex<Option<oneshot::Sender<Result<PhoneState, String>>>>,
}

impl PhoneStateSink {
  pub fn channel() -> (Arc<Self>, oneshot::Receiver<Result<PhoneState, String>>) {
    let (tx, rx) = oneshot::channel();
    (
      Arc::new(Self {
        tx: std::sync::Mutex::new(Some(tx)),
      }),
      rx,
    )
  }

  fn settle(&self, result: Result<PhoneState, String>) {
    if let Some(tx) = self.tx.lock().unwrap().take() {
      let _ = tx.send(result);
    }
  }
}

#[uniffi::export]
impl PhoneStateSink {
  pub fn complete(&self, state: PhoneState) {
    self.settle(Ok(state));
  }

  pub fn fail(&self, reason: String) {
    self.settle(Err(reason));
  }
}
