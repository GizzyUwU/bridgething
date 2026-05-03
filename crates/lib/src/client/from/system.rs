use bridgething_macros::{BridgeEnum, WireRequest};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Marker request: webapp asks the bridge for its `BridgeThingMeta`.
/// Pairs with `BridgeToClientSystemMsg::Version`.
#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = ClientToBridge,
  surface = System,
  request_variant = VersionRequest,
  response = crate::BridgeThingMeta,
  response_variant = Version,
  boxed_response,
)]
pub struct RequestVersion;

/// Marker request: webapp asks the bridge for the current `GatewayStatus`.
/// Pairs with `BridgeToClientSystemMsg::GatewayStatus`.
#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = ClientToBridge,
  surface = System,
  request_variant = GatewayStatusRequest,
  response = crate::client::GatewayStatus,
  response_variant = GatewayStatus,
)]
pub struct RequestGatewayStatus;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PhoneCallAccept {
  pub call_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PhoneCallEnd {
  pub call_id: String,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::ClientToBridgeMsgData)]
pub enum ClientToBridgeSystemMsg {
  #[bridge_request]
  VersionRequest,
  #[bridge_request]
  GatewayStatusRequest,
  #[bridge_command]
  Reboot,
  #[bridge_command]
  PowerOff,
  #[bridge_command]
  FactoryReset,
  #[bridge_command]
  PhoneCallAccept(PhoneCallAccept),
  #[bridge_command]
  PhoneCallEnd(PhoneCallEnd),
}
