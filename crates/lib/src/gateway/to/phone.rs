use bridgething_macros::{BridgeEnum, WireRequest};
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct PhoneCallAction {
  pub call_id: String,
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

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::BridgeToGatewayMsgData)]
pub enum BridgeToGatewayPhoneMsg {
  #[bridge_command]
  Answer(PhoneCallAction),
  #[bridge_command]
  Decline(PhoneCallAction),
  #[bridge_command]
  End(PhoneCallAction),
  #[bridge_command]
  Hold(PhoneCallAction),
  #[bridge_command]
  Unhold(PhoneCallAction),
  #[bridge_request]
  StateGet,
}
