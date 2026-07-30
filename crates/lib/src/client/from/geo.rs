use bridgething_macros::{BridgeDispatch, BridgeEnum, WireRequest};
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
/// Payload for `geo.watch`: register a standing subscription for
/// phone-sourced position fixes. The daemon aggregates every active
/// watcher's `accuracy` and `min_interval_ms` and forwards the most
/// demanding combination to the companion.
pub struct GeoWatch {
  pub accuracy: GeoAccuracy,
  /// Minimum time between fixes, in milliseconds. The daemon uses the
  /// smallest value across all active watchers.
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

#[serde_with::skip_serializing_none]
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
/// Payload for `geo.getOnce`: fetch a single phone-sourced position
/// fix without registering a standing watch.
pub struct GeoGetOnce {
  pub accuracy: GeoAccuracy,
  /// Largest acceptable age, in seconds, for an already-held fix. Absent or
  /// `0` forces a fresh fix from the phone.
  #[serde(default)]
  pub max_age_s: Option<u32>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum, BridgeDispatch)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::ClientToBridgeMsgData)]
/// Webapp -> daemon location surface: `watch`/`unwatch` register and
/// release a standing subscription, `getOnce` fetches a single fix.
/// Every fix originates from the connected phone; the device has no
/// GPS of its own.
pub enum ClientToBridgeGeoMsg {
  #[bridge_request]
  Watch(GeoWatch),
  #[bridge_command]
  Unwatch(GeoUnwatch),
  #[bridge_request]
  GetOnce(GeoGetOnce),
}
