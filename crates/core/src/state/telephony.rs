//! Telephony manager. Tracks the most recent CallStateUpdate and
//! CommunicationsUpdate snapshots received from the iAP2 control
//! session, and dispatches outbound action CSMs (Initiate/Accept/End/
//! Swap/Merge/Hold/Mute/DTMF) through the iAP2 manager when a session
//! is identified.
//!
//! State is intentionally minimal - we hold what arrived but don't
//! interpret it; webapps subscribed to the Phone SDK surface receive
//! delta events and read the merged snapshot through `state.get`.

use std::{collections::HashMap, sync::Arc};

use bridgething_iap2::{
  csm::telephony::{
    AcceptCall as Iap2AcceptCall, CallStateUpdate as Iap2CallStateUpdate,
    CommunicationsUpdate as Iap2CommunicationsUpdate, EndCall as Iap2EndCall, HoldStatusUpdate as Iap2HoldStatusUpdate,
    InitiateCall as Iap2InitiateCall, MuteStatusUpdate as Iap2MuteStatusUpdate, SendDtmf as Iap2SendDtmf,
  },
  session::TelephonyCommand,
};
use libbridgething::{
  CallEndReason, CommunicationsState, DtmfTone, PhoneCall, PhoneCallDirection, PhoneCallService, PhoneCallStatus,
  PhoneState, RegistrationStatus,
  client::{BridgeToClientPhoneMsg, PhoneCallEnded, PhoneCommunicationsReply, PhoneStateReply},
  wire::MsgMeta,
};
use tokio::sync::RwLock;

use crate::{bluetooth::iap2::Iap2TelephonyHandle, net::WireEventBus};

#[derive(Debug, Default)]
struct Inner {
  calls: HashMap<String, PhoneCall>,
  communications: CommunicationsState,
}

#[derive(Debug, Clone)]
pub struct TelephonyManager {
  inner: Arc<RwLock<Inner>>,
  bus: WireEventBus,
  iap2: Option<Iap2TelephonyHandle>,
}

#[derive(Debug, thiserror::Error)]
pub enum TelephonyError {
  #[error("no iAP2 link to send action")]
  NoIap2,
  #[error("broadcast failed for {0} client(s)")]
  Broadcast(usize),
}

impl TelephonyManager {
  pub fn new(bus: WireEventBus, iap2: Option<Iap2TelephonyHandle>) -> Self {
    Self {
      inner: Arc::new(RwLock::new(Inner::default())),
      bus,
      iap2,
    }
  }

  pub async fn snapshot(&self) -> PhoneState {
    let inner = self.inner.read().await;
    PhoneState {
      active_calls: inner.calls.values().cloned().collect(),
    }
  }

  pub async fn communications(&self) -> CommunicationsState {
    self.inner.read().await.communications.clone()
  }

  pub async fn apply_iap2_call_state(&self, update: Iap2CallStateUpdate) -> Result<(), TelephonyError> {
    let Some(call_id) = update.call_uuid.clone() else {
      tracing::debug!(
        ?update,
        "iap2 call-state update without CallUUID (no active call snapshot)"
      );
      return Ok(());
    };

    let mut inner = self.inner.write().await;
    let entry = inner.calls.entry(call_id.clone()).or_insert_with(|| PhoneCall {
      call_id: call_id.clone(),
      remote_id: String::new(),
      display_name: String::new(),
      status: PhoneCallStatus::Disconnected,
      direction: PhoneCallDirection::Incoming,
      started_at_unix_s: None,
      label: None,
      address_book_id: None,
      service: None,
      is_conferenced: None,
      conference_group: None,
    });
    if let Some(remote_id) = update.remote_id {
      entry.remote_id = remote_id;
    }
    if let Some(display_name) = update.display_name {
      entry.display_name = display_name;
    }
    if let Some(status) = update.status {
      entry.status = decode_status(status);
    }
    if let Some(direction) = update.direction {
      entry.direction = decode_direction(direction);
    }
    if let Some(label) = update.label {
      entry.label = Some(label);
    }
    if let Some(address_book_id) = update.address_book_id {
      entry.address_book_id = Some(address_book_id);
    }
    if let Some(service) = update.service {
      entry.service = Some(decode_service(service));
    }
    if let Some(is_conferenced) = update.is_conferenced {
      entry.is_conferenced = Some(is_conferenced);
    }
    if let Some(conference_group) = update.conference_group {
      entry.conference_group = Some(conference_group);
    }
    if let Some(start_ts) = update.start_timestamp_unix_s {
      entry.started_at_unix_s = u32::try_from(start_ts).ok();
    }
    let snapshot = entry.clone();
    drop(inner);

    self.broadcast(BridgeToClientPhoneMsg::CallUpdated(snapshot)).await
  }

  /// Replace the active-call set with the companion's announce-time
  /// snapshot. No per-call event is fanned out - webapps that connect
  /// later read the merged state via `phone.stateGet`.
  pub async fn apply_companion_snapshot(&self, state: PhoneState) -> Result<(), TelephonyError> {
    let mut inner = self.inner.write().await;
    inner.calls = state.active_calls.into_iter().map(|c| (c.call_id.clone(), c)).collect();
    Ok(())
  }

  pub async fn apply_companion_call_started(&self, call: PhoneCall) -> Result<(), TelephonyError> {
    let snapshot = call.clone();
    {
      let mut inner = self.inner.write().await;
      inner.calls.insert(call.call_id.clone(), call);
    }
    self.broadcast(BridgeToClientPhoneMsg::CallStarted(snapshot)).await
  }

  pub async fn apply_companion_call_updated(&self, call: PhoneCall) -> Result<(), TelephonyError> {
    let snapshot = call.clone();
    {
      let mut inner = self.inner.write().await;
      inner.calls.insert(call.call_id.clone(), call);
    }
    self.broadcast(BridgeToClientPhoneMsg::CallUpdated(snapshot)).await
  }

  pub async fn apply_companion_call_ended(&self, call_id: String, reason: CallEndReason) -> Result<(), TelephonyError> {
    {
      let mut inner = self.inner.write().await;
      inner.calls.remove(&call_id);
    }
    self
      .broadcast(BridgeToClientPhoneMsg::CallEnded(PhoneCallEnded { call_id, reason }))
      .await
  }

  pub async fn apply_companion_communications(&self, state: CommunicationsState) -> Result<(), TelephonyError> {
    {
      let mut inner = self.inner.write().await;
      inner.communications = state;
    }
    let snapshot = self.communications().await;
    self
      .broadcast(BridgeToClientPhoneMsg::CommunicationsChanged(
        PhoneCommunicationsReply { state: snapshot },
      ))
      .await
  }

  pub async fn apply_iap2_communications(&self, update: Iap2CommunicationsUpdate) -> Result<(), TelephonyError> {
    {
      let mut inner = self.inner.write().await;
      let comm = &mut inner.communications;
      if let Some(v) = update.signal_strength {
        comm.signal_strength = Some(v);
      }
      if let Some(v) = update.registration_status {
        comm.registration_status = Some(decode_registration(v));
      }
      if let Some(v) = update.airplane_mode {
        comm.airplane_mode = Some(v);
      }
      if let Some(v) = update.carrier_name {
        comm.carrier_name = Some(v);
      }
      if let Some(v) = update.cellular_supported {
        comm.cellular_supported = Some(v);
      }
      if let Some(v) = update.telephony_enabled {
        comm.telephony_enabled = Some(v);
      }
      if let Some(v) = update.face_time_audio_enabled {
        comm.face_time_audio_enabled = Some(v);
      }
      if let Some(v) = update.face_time_video_enabled {
        comm.face_time_video_enabled = Some(v);
      }
      if let Some(v) = update.mute_status {
        comm.mute_status = Some(v);
      }
      if let Some(v) = update.current_call_count {
        comm.current_call_count = Some(v);
      }
      if let Some(v) = update.new_voicemail_count {
        comm.new_voicemail_count = Some(v);
      }
      if let Some(v) = update.initiate_call_available {
        comm.initiate_call_available = Some(v);
      }
      if let Some(v) = update.end_and_accept_available {
        comm.end_and_accept_available = Some(v);
      }
      if let Some(v) = update.hold_and_accept_available {
        comm.hold_and_accept_available = Some(v);
      }
      if let Some(v) = update.swap_available {
        comm.swap_available = Some(v);
      }
      if let Some(v) = update.merge_available {
        comm.merge_available = Some(v);
      }
      if let Some(v) = update.hold_available {
        comm.hold_available = Some(v);
      }
    }
    let state = self.communications().await;
    self
      .broadcast(BridgeToClientPhoneMsg::CommunicationsChanged(
        PhoneCommunicationsReply { state },
      ))
      .await
  }

  pub async fn dispatch(&self, cmd: TelephonyCommand) -> Result<(), TelephonyError> {
    let Some(handle) = self.iap2.as_ref() else {
      return Err(TelephonyError::NoIap2);
    };
    handle.send(cmd).await;
    Ok(())
  }

  /// Helpers for the handler layer to build typed iAP2 actions without
  /// importing the iap2 crate's types directly.
  pub fn build_initiate(
    kind: u8,
    destination_id: Option<String>,
    service: Option<u8>,
    address_book_id: Option<String>,
  ) -> TelephonyCommand {
    TelephonyCommand::Initiate(Iap2InitiateCall {
      kind,
      destination_id,
      service,
      address_book_id,
    })
  }

  pub fn build_accept(action: u8, call_uuid: Option<String>) -> TelephonyCommand {
    TelephonyCommand::Accept(Iap2AcceptCall {
      accept_action: action,
      call_uuid,
    })
  }

  pub fn build_end(action: u8, call_uuid: Option<String>) -> TelephonyCommand {
    TelephonyCommand::End(Iap2EndCall {
      end_action: action,
      call_uuid,
    })
  }

  pub fn build_hold(hold: bool, call_uuid: Option<String>) -> TelephonyCommand {
    TelephonyCommand::Hold(Iap2HoldStatusUpdate {
      hold_status: hold,
      call_uuid,
    })
  }

  pub fn build_mute(mute: bool) -> TelephonyCommand {
    TelephonyCommand::Mute(Iap2MuteStatusUpdate { mute_status: mute })
  }

  pub fn build_dtmf(tone: DtmfTone, call_uuid: Option<String>) -> TelephonyCommand {
    TelephonyCommand::Dtmf(Iap2SendDtmf {
      tone: encode_dtmf_tone(tone),
      call_uuid,
    })
  }

  async fn broadcast(&self, event: BridgeToClientPhoneMsg) -> Result<(), TelephonyError> {
    if let Err(errors) = self.bus.broadcast(event, MsgMeta::Event).await {
      return Err(TelephonyError::Broadcast(errors.len()));
    }
    Ok(())
  }

  pub async fn snapshot_reply(&self) -> PhoneStateReply {
    PhoneStateReply {
      state: self.snapshot().await,
    }
  }
}

fn decode_status(byte: u8) -> PhoneCallStatus {
  match byte {
    1 => PhoneCallStatus::Sending,
    2 => PhoneCallStatus::Ringing,
    3 => PhoneCallStatus::Connecting,
    4 => PhoneCallStatus::Active,
    5 => PhoneCallStatus::Held,
    6 => PhoneCallStatus::Disconnecting,
    _ => PhoneCallStatus::Disconnected,
  }
}

fn decode_direction(byte: u8) -> PhoneCallDirection {
  match byte {
    1 => PhoneCallDirection::Incoming,
    2 => PhoneCallDirection::Outgoing,
    _ => PhoneCallDirection::Incoming,
  }
}

fn decode_service(byte: u8) -> PhoneCallService {
  match byte {
    1 => PhoneCallService::Telephony,
    2 => PhoneCallService::FaceTimeAudio,
    3 => PhoneCallService::FaceTimeVideo,
    _ => PhoneCallService::Unknown,
  }
}

fn decode_registration(byte: u8) -> RegistrationStatus {
  match byte {
    1 => RegistrationStatus::NotRegistered,
    2 => RegistrationStatus::Searching,
    3 => RegistrationStatus::Denied,
    4 => RegistrationStatus::RegisteredHome,
    5 => RegistrationStatus::RegisteredRoaming,
    6 => RegistrationStatus::EmergencyCallsOnly,
    _ => RegistrationStatus::Unknown,
  }
}

fn encode_dtmf_tone(tone: DtmfTone) -> u8 {
  match tone {
    DtmfTone::D0 => 0,
    DtmfTone::D1 => 1,
    DtmfTone::D2 => 2,
    DtmfTone::D3 => 3,
    DtmfTone::D4 => 4,
    DtmfTone::D5 => 5,
    DtmfTone::D6 => 6,
    DtmfTone::D7 => 7,
    DtmfTone::D8 => 8,
    DtmfTone::D9 => 9,
    DtmfTone::Star => 10,
    DtmfTone::Hash => 11,
  }
}
