use bridgething_macros::{BridgeEnum, WireRequest};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PhoneCallAction {
  pub call_id: String,
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
