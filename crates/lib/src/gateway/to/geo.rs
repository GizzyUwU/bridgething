use bridgething_macros::{BridgeEnum, WireRequest};
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

use crate::GeoAccuracy;

/// Bridge → companion watch forward. The daemon aggregates webapp
/// watches and re-issues this with the most-demanding accuracy +
/// fastest interval. `min_interval_ms = 0` lets the gateway pick.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct GeoWatch {
  pub accuracy: GeoAccuracy,
  pub min_interval_ms: u32,
}

#[typeshare]
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

#[typeshare]
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
