use bridgething_macros::{BridgeEnum, WireRequest};
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::GeoAccuracy;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct GeoWatch {
  pub accuracy: GeoAccuracy,
  pub min_interval_ms: u32,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = BridgeToGateway,
  surface = Geo,
  request_variant = GetOnce,
  response = crate::gateway::GeoGetOnceReply,
  response_variant = GetOnceReply,
  error = crate::gateway::GeoErrorReply,
  error_variant = ErrorReply,
)]
pub struct GeoGetOnce {
  pub accuracy: GeoAccuracy,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::BridgeToGatewayMsgData)]
pub enum BridgeToGatewayGeoMsg {
  #[bridge_command]
  Watch(GeoWatch),
  #[bridge_command]
  Unwatch,
  #[bridge_request]
  GetOnce(GeoGetOnce),
}
