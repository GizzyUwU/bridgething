use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
  BridgeThingMeta,
  client::ClientCommandType,
  impl_client_request,
  server::{GatewayStatus, ServerEventData, ServerSystemEvent},
};

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(
  tag = "action",
  content = "args",
  rename_all = "camelCase",
  rename_all_fields = "camelCase"
)]
#[ts(export, export_to = "client.ts")]
pub enum ClientSystemCommand {
  VersionRequest,
  GatewayStatusRequest,

  Reboot,
  PowerOff,
  FactoryReset,

  PhoneCallAccept { call_id: String },
  PhoneCallEnd { call_id: String },
}

/// Marker request: webapp asks the bridge for its `BridgeThingMeta`.
/// Pairs with `ServerSystemEvent::Version`.
#[derive(Debug, Clone, Copy, Default)]
pub struct RequestVersion;

/// Marker request: webapp asks the bridge for the current `GatewayStatus`.
/// Pairs with `ServerSystemEvent::GatewayStatus`.
#[derive(Debug, Clone, Copy, Default)]
pub struct RequestGatewayStatus;

impl_client_request! {
  request: RequestVersion,
  response: BridgeThingMeta,
  encode_request:
    _r => ClientCommandType::System(ClientSystemCommand::VersionRequest),
  extract_response:
    ServerEventData::System(ServerSystemEvent::Version(v)) => v,
  encode_response:
    v => ServerEventData::System(ServerSystemEvent::Version(v)),
}

impl_client_request! {
  request: RequestGatewayStatus,
  response: GatewayStatus,
  encode_request:
    _r => ClientCommandType::System(ClientSystemCommand::GatewayStatusRequest),
  extract_response:
    ServerEventData::System(ServerSystemEvent::GatewayStatus(v)) => v,
  encode_response:
    v => ServerEventData::System(ServerSystemEvent::GatewayStatus(v)),
}
