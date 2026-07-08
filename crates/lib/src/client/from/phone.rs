use bridgething_macros::{BridgeDispatch, BridgeEnum, WireRequest};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{AcceptCallAction, DtmfTone, EndCallAction, InitiateCallType, PhoneCallService};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Payload for `answer`, `decline`, `end`, `hold`, and `unhold`: the call to act on.
pub struct PhoneCallAction {
  /// Target call, as surfaced on `PhoneCall.call_id`.
  pub call_id: String,
}

/// Explicit-action variant of `Answer`. `Accept` (default) places any
/// existing active call on hold; `EndAndAccept` ends the existing call
/// first. Webapps gate on `CommunicationsState.hold_and_accept_available`
/// or `end_and_accept_available` before sending the non-default action.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PhoneAcceptAction {
  /// Target call, as surfaced on `PhoneCall.call_id`.
  pub call_id: String,
  pub action: AcceptCallAction,
}

/// Explicit-action variant of `End`. `End` (default) ends the named
/// call; `EndAll` ends every active call (multi-call / conference).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PhoneEndAction {
  /// Target call, as surfaced on `PhoneCall.call_id`.
  pub call_id: String,
  pub action: EndCallAction,
}

/// Payload for `initiate`: place an outbound call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PhoneInitiateAction {
  /// What kind of outbound call to place.
  pub kind: InitiateCallType,
  /// Destination address (e.g. phone number) for `Destination` calls; ignored for `Voicemail`/`Redial`.
  pub destination_id: Option<String>,
  /// Call bearer to use; `None` lets the companion pick its default.
  pub service: Option<PhoneCallService>,
  /// Contact id to associate with the call, when known.
  pub address_book_id: Option<String>,
}

/// Payload for `mute`: mic-mute state for the active call.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PhoneMuteAction {
  /// `true` mutes the mic, `false` unmutes.
  pub mute: bool,
}

/// Payload for `dtmf`: play a tone on an active call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PhoneDtmfAction {
  /// Call to send the tone on; `None` targets the active call.
  pub call_id: Option<String>,
  pub tone: DtmfTone,
}

/// Webapp asks for the current `PhoneState` snapshot.
#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = ClientToBridge,
  surface = Phone,
  request_variant = StateGet,
  response = crate::client::PhoneStateReply,
  response_variant = StateReply,
)]
pub struct PhoneStateGet;

/// Webapp -> daemon call-control surface. Commands are fire-and-forget and
/// route to the connected companion's telephony backend (iAP2 call CSMs on
/// iOS, gateway telephony on Android); outcomes surface asynchronously via
/// `BridgeToClientPhoneMsg` events.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum, BridgeDispatch)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::ClientToBridgeMsgData)]
pub enum ClientToBridgePhoneMsg {
  #[bridge_command]
  Answer(PhoneCallAction),
  #[bridge_command]
  Accept(PhoneAcceptAction),
  #[bridge_command]
  Decline(PhoneCallAction),
  #[bridge_command]
  End(PhoneCallAction),
  #[bridge_command]
  EndTyped(PhoneEndAction),
  #[bridge_command]
  Hold(PhoneCallAction),
  #[bridge_command]
  Unhold(PhoneCallAction),
  #[bridge_command]
  Initiate(PhoneInitiateAction),
  #[bridge_command]
  Swap,
  #[bridge_command]
  Merge,
  #[bridge_command]
  Mute(PhoneMuteAction),
  #[bridge_command]
  Dtmf(PhoneDtmfAction),
  #[bridge_request]
  StateGet,
}
