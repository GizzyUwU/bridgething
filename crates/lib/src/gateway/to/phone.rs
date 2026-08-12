use bridgething_macros::{BridgeEnum, WireRequest};
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{AcceptCallAction, DtmfTone, EndCallAction, InitiateCallType, PhoneCallService};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct PhoneCallAction {
  pub call_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct PhoneAcceptAction {
  pub call_id: String,
  pub action: AcceptCallAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct PhoneEndAction {
  pub call_id: String,
  pub action: EndCallAction,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct PhoneInitiateAction {
  pub kind: InitiateCallType,
  pub destination_id: Option<String>,
  pub service: Option<PhoneCallService>,
  pub address_book_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct PhoneMuteAction {
  pub mute: bool,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct PhoneDtmfAction {
  pub call_id: Option<String>,
  pub tone: DtmfTone,
}

#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = BridgeToGateway,
  surface = Phone,
  request_variant = StateGet,
  response = crate::gateway::PhoneStateReply,
  response_variant = StateReply,
)]
pub struct PhoneStateGet;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::BridgeToGatewayMsgData)]
pub enum BridgeToGatewayPhoneMsg {
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
