use bridgething_macros::{BridgeEnum, WireRequest};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{AcceptCallAction, DtmfTone, EndCallAction, InitiateCallType, PhoneCallService};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PhoneCallAction {
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
  pub call_id: String,
  pub action: AcceptCallAction,
}

/// Explicit-action variant of `End`. `End` (default) ends the named
/// call; `EndAll` ends every active call (multi-call / conference).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PhoneEndAction {
  pub call_id: String,
  pub action: EndCallAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PhoneInitiateAction {
  pub kind: InitiateCallType,
  pub destination_id: Option<String>,
  pub service: Option<PhoneCallService>,
  pub address_book_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PhoneMuteAction {
  pub mute: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PhoneDtmfAction {
  pub call_id: Option<String>,
  pub tone: DtmfTone,
}

#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = ClientToBridge,
  surface = Phone,
  request_variant = StateGet,
  response = crate::client::PhoneStateReply,
  response_variant = StateReply,
)]
pub struct PhoneStateGet;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
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
