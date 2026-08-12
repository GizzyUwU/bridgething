use std::{sync::Arc, time::Duration};

use bridgething_gateway::{HandlerError, OutboundLink, OutboundLinkExt, PhoneHandler, Reply};
use libbridgething::{
  AcceptCallAction as WireAccept, CallEndReason as WireEndReason, CommunicationsState as WireCommunications,
  DtmfTone as WireTone, EndCallAction as WireEnd, InitiateCallType as WireInitiateKind, PhoneCall as WireCall,
  PhoneCallDirection as WireDirection, PhoneCallService as WireService, PhoneCallStatus as WireStatus,
  PhoneState as WireState, RegistrationStatus as WireRegistration,
  gateway::{
    CommunicationsSnapshot, GatewayToBridgePhoneMsgEvent, PhoneAcceptAction, PhoneCallAction,
    PhoneCallEnded as WireCallEnded, PhoneDtmfAction, PhoneEndAction, PhoneInitiateAction, PhoneMuteAction,
    PhoneStateReply,
  },
  wire::WireError,
};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::{
  backend::{
    AcceptCallAction, CallEndReason, CommunicationsState, DtmfTone, EndCallAction, InitiateCallType, PhoneBackend,
    PhoneCall, PhoneCallDirection, PhoneCallEnded, PhoneCallService, PhoneCallStatus, PhoneCommand, PhoneEvent,
    PhoneInbox, PhoneInitiate, PhoneState, PhoneStateSink, RegistrationStatus,
  },
  dispatch::{Relay, tell},
};

const STATE_DEADLINE: Duration = Duration::from_secs(5);

pub struct PhoneDispatcher {
  backend: Arc<dyn PhoneBackend>,
  link: Arc<dyn OutboundLink>,
  relay: Relay,
}

impl PhoneDispatcher {
  pub fn new(backend: Arc<dyn PhoneBackend>, link: Arc<dyn OutboundLink>) -> Self {
    Self {
      backend,
      link,
      relay: Relay::default(),
    }
  }

  pub async fn start(&self) {
    let (inbox, events) = PhoneInbox::channel();
    self.relay.hold(tokio::spawn(relay(events, self.link.clone())));
    tell(&self.backend, move |backend| backend.start(inbox)).await;
  }

  pub async fn stop(&self) {
    self.relay.release();
    tell(&self.backend, |backend| backend.stop()).await;
  }

  pub async fn announce(&self) {
    match self.state().await {
      Ok(state) => {
        let _ = self
          .link
          .event(GatewayToBridgePhoneMsgEvent::Snapshot(PhoneStateReply { state }))
          .await;
      }
      Err(reason) => tracing::warn!(%reason, "phone state unavailable on connect"),
    }
  }

  async fn state(&self) -> Result<WireState, String> {
    let (sink, answer) = PhoneStateSink::channel();
    tell(&self.backend, move |backend| backend.state_get(sink)).await;
    match tokio::time::timeout(STATE_DEADLINE, answer).await {
      Ok(Ok(Ok(state))) => Ok(wire_state(state)),
      Ok(Ok(Err(reason))) => Err(reason),
      Ok(Err(_)) => Err("the platform abandoned the phone state request".into()),
      Err(_) => Err("the platform did not answer the phone state request".into()),
    }
  }

  async fn command(&self, cmd: PhoneCommand) -> Result<(), WireError> {
    tell(&self.backend, move |backend| backend.command(cmd)).await;
    Ok(())
  }
}

impl PhoneHandler for PhoneDispatcher {
  async fn state_get(&self) -> Result<Reply<PhoneStateReply>, HandlerError<std::convert::Infallible>> {
    match self.state().await {
      Ok(state) => Ok(PhoneStateReply { state }.into()),
      Err(reason) => Err(HandlerError::Wire(WireError::HandlerFailed { reason })),
    }
  }

  async fn answer(&self, payload: PhoneCallAction) -> Result<(), WireError> {
    self
      .command(PhoneCommand::Answer {
        call_id: payload.call_id,
      })
      .await
  }

  async fn accept(&self, payload: PhoneAcceptAction) -> Result<(), WireError> {
    self
      .command(PhoneCommand::Accept {
        call_id: payload.call_id,
        action: accept_action(payload.action),
      })
      .await
  }

  async fn decline(&self, payload: PhoneCallAction) -> Result<(), WireError> {
    self
      .command(PhoneCommand::Decline {
        call_id: payload.call_id,
      })
      .await
  }

  async fn end(&self, payload: PhoneCallAction) -> Result<(), WireError> {
    self
      .command(PhoneCommand::End {
        call_id: payload.call_id,
      })
      .await
  }

  async fn end_typed(&self, payload: PhoneEndAction) -> Result<(), WireError> {
    self
      .command(PhoneCommand::EndTyped {
        call_id: payload.call_id,
        action: end_action(payload.action),
      })
      .await
  }

  async fn hold(&self, payload: PhoneCallAction) -> Result<(), WireError> {
    self
      .command(PhoneCommand::Hold {
        call_id: payload.call_id,
      })
      .await
  }

  async fn unhold(&self, payload: PhoneCallAction) -> Result<(), WireError> {
    self
      .command(PhoneCommand::Unhold {
        call_id: payload.call_id,
      })
      .await
  }

  async fn initiate(&self, payload: PhoneInitiateAction) -> Result<(), WireError> {
    self
      .command(PhoneCommand::Initiate {
        action: PhoneInitiate {
          kind: initiate_kind(payload.kind),
          destination_id: payload.destination_id,
          service: payload.service.map(service),
          address_book_id: payload.address_book_id,
        },
      })
      .await
  }

  async fn swap(&self) -> Result<(), WireError> {
    self.command(PhoneCommand::Swap).await
  }

  async fn merge(&self) -> Result<(), WireError> {
    self.command(PhoneCommand::Merge).await
  }

  async fn mute(&self, payload: PhoneMuteAction) -> Result<(), WireError> {
    self.command(PhoneCommand::Mute { muted: payload.mute }).await
  }

  async fn dtmf(&self, payload: PhoneDtmfAction) -> Result<(), WireError> {
    self
      .command(PhoneCommand::Dtmf {
        call_id: payload.call_id,
        tone: tone(payload.tone),
      })
      .await
  }
}

async fn relay(mut events: UnboundedReceiver<PhoneEvent>, link: Arc<dyn OutboundLink>) {
  while let Some(event) = events.recv().await {
    let _ = match event {
      PhoneEvent::CallStarted(call) => {
        link
          .event(GatewayToBridgePhoneMsgEvent::CallStarted(wire_call(call)))
          .await
      }
      PhoneEvent::CallUpdated(call) => {
        link
          .event(GatewayToBridgePhoneMsgEvent::CallUpdated(wire_call(call)))
          .await
      }
      PhoneEvent::CallEnded(ended) => {
        link
          .event(GatewayToBridgePhoneMsgEvent::CallEnded(wire_call_ended(ended)))
          .await
      }
      PhoneEvent::State(state) => {
        link
          .event(GatewayToBridgePhoneMsgEvent::Snapshot(PhoneStateReply {
            state: wire_state(state),
          }))
          .await
      }
      PhoneEvent::Communications(state) => {
        link
          .event(GatewayToBridgePhoneMsgEvent::CommunicationsSnapshot(
            CommunicationsSnapshot {
              state: wire_communications(state),
            },
          ))
          .await
      }
    };
  }
}

fn accept_action(action: WireAccept) -> AcceptCallAction {
  match action {
    WireAccept::Accept => AcceptCallAction::Accept,
    WireAccept::EndAndAccept => AcceptCallAction::EndAndAccept,
  }
}

fn end_action(action: WireEnd) -> EndCallAction {
  match action {
    WireEnd::End => EndCallAction::End,
    WireEnd::EndAll => EndCallAction::EndAll,
  }
}

fn initiate_kind(kind: WireInitiateKind) -> InitiateCallType {
  match kind {
    WireInitiateKind::Destination => InitiateCallType::Destination,
    WireInitiateKind::Voicemail => InitiateCallType::Voicemail,
    WireInitiateKind::Redial => InitiateCallType::Redial,
  }
}

fn service(service: WireService) -> PhoneCallService {
  match service {
    WireService::Unknown => PhoneCallService::Unknown,
    WireService::Telephony => PhoneCallService::Telephony,
    WireService::FaceTimeAudio => PhoneCallService::FaceTimeAudio,
    WireService::FaceTimeVideo => PhoneCallService::FaceTimeVideo,
  }
}

fn tone(tone: WireTone) -> DtmfTone {
  match tone {
    WireTone::D0 => DtmfTone::D0,
    WireTone::D1 => DtmfTone::D1,
    WireTone::D2 => DtmfTone::D2,
    WireTone::D3 => DtmfTone::D3,
    WireTone::D4 => DtmfTone::D4,
    WireTone::D5 => DtmfTone::D5,
    WireTone::D6 => DtmfTone::D6,
    WireTone::D7 => DtmfTone::D7,
    WireTone::D8 => DtmfTone::D8,
    WireTone::D9 => DtmfTone::D9,
    WireTone::Star => DtmfTone::Star,
    WireTone::Hash => DtmfTone::Hash,
  }
}

fn wire_state(state: PhoneState) -> WireState {
  WireState {
    active_calls: state.active_calls.into_iter().map(wire_call).collect(),
  }
}

fn wire_call(call: PhoneCall) -> WireCall {
  WireCall {
    call_id: call.call_id,
    remote_id: call.remote_id,
    display_name: call.display_name,
    status: wire_status(call.status),
    direction: match call.direction {
      PhoneCallDirection::Incoming => WireDirection::Incoming,
      PhoneCallDirection::Outgoing => WireDirection::Outgoing,
    },
    started_at_unix_s: call.started_at_unix_s,
    label: call.label,
    address_book_id: call.address_book_id,
    service: call.service.map(wire_service),
    is_conferenced: call.is_conferenced,
    conference_group: call.conference_group,
  }
}

fn wire_status(status: PhoneCallStatus) -> WireStatus {
  match status {
    PhoneCallStatus::Disconnected => WireStatus::Disconnected,
    PhoneCallStatus::Sending => WireStatus::Sending,
    PhoneCallStatus::Ringing => WireStatus::Ringing,
    PhoneCallStatus::Connecting => WireStatus::Connecting,
    PhoneCallStatus::Active => WireStatus::Active,
    PhoneCallStatus::Held => WireStatus::Held,
    PhoneCallStatus::Disconnecting => WireStatus::Disconnecting,
  }
}

fn wire_service(service: PhoneCallService) -> WireService {
  match service {
    PhoneCallService::Unknown => WireService::Unknown,
    PhoneCallService::Telephony => WireService::Telephony,
    PhoneCallService::FaceTimeAudio => WireService::FaceTimeAudio,
    PhoneCallService::FaceTimeVideo => WireService::FaceTimeVideo,
  }
}

fn wire_call_ended(ended: PhoneCallEnded) -> WireCallEnded {
  WireCallEnded {
    call_id: ended.call_id,
    reason: match ended.reason {
      CallEndReason::Local => WireEndReason::Local,
      CallEndReason::Remote => WireEndReason::Remote,
      CallEndReason::Missed => WireEndReason::Missed,
      CallEndReason::Declined => WireEndReason::Declined,
      CallEndReason::Failed { reason } => WireEndReason::Failed { reason },
    },
  }
}

fn wire_communications(state: CommunicationsState) -> WireCommunications {
  WireCommunications {
    signal_strength: state.signal_strength,
    registration_status: state.registration_status.map(wire_registration),
    airplane_mode: state.airplane_mode,
    carrier_name: state.carrier_name,
    cellular_supported: state.cellular_supported,
    telephony_enabled: state.telephony_enabled,
    face_time_audio_enabled: state.face_time_audio_enabled,
    face_time_video_enabled: state.face_time_video_enabled,
    mute_status: state.mute_status,
    current_call_count: state.current_call_count,
    new_voicemail_count: state.new_voicemail_count,
    initiate_call_available: state.initiate_call_available,
    end_and_accept_available: state.end_and_accept_available,
    hold_and_accept_available: state.hold_and_accept_available,
    swap_available: state.swap_available,
    merge_available: state.merge_available,
    hold_available: state.hold_available,
  }
}

fn wire_registration(status: RegistrationStatus) -> WireRegistration {
  match status {
    RegistrationStatus::Unknown => WireRegistration::Unknown,
    RegistrationStatus::NotRegistered => WireRegistration::NotRegistered,
    RegistrationStatus::Searching => WireRegistration::Searching,
    RegistrationStatus::Denied => WireRegistration::Denied,
    RegistrationStatus::RegisteredHome => WireRegistration::RegisteredHome,
    RegistrationStatus::RegisteredRoaming => WireRegistration::RegisteredRoaming,
    RegistrationStatus::EmergencyCallsOnly => WireRegistration::EmergencyCallsOnly,
  }
}
