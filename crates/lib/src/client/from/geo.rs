use bridgething_macros::{BridgeEnum, WireRequest};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::GeoAccuracy;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[wire_request(
  direction = ClientToBridge,
  surface = Geo,
  request_variant = Watch,
  response = crate::client::GeoWatchReply,
  response_variant = WatchReply,
  error = crate::client::GeoErrorReply,
  error_variant = ErrorReply,
)]
pub struct GeoWatch {
  pub accuracy: GeoAccuracy,
  pub min_interval_ms: u32,
}

/// Stop a previously-issued watch. `token` is the value returned in
/// `GeoWatchReply.token`. The daemon refcounts watches across webapps;
/// the companion gets `Unwatch` only when the last token is released.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct GeoUnwatch {
  pub token: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[wire_request(
  direction = ClientToBridge,
  surface = Geo,
  request_variant = GetOnce,
  response = crate::client::GeoGetOnceReply,
  response_variant = GetOnceReply,
  error = crate::client::GeoErrorReply,
  error_variant = ErrorReply,
)]
pub struct GeoGetOnce {
  pub accuracy: GeoAccuracy,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::ClientToBridgeMsgData)]
pub enum ClientToBridgeGeoMsg {
  #[bridge_request]
  Watch(GeoWatch),
  #[bridge_command]
  Unwatch(GeoUnwatch),
  #[bridge_request]
  GetOnce(GeoGetOnce),
}
