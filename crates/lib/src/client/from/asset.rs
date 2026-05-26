use bridgething_macros::{BridgeDispatch, BridgeEnum, WireRequest};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// Webapp request: read an asset by id. Bridge replies with `Got` on hit
/// or `NotFound` (domain) when neither cache nor companion has it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[wire_request(
  direction = ClientToBridge,
  surface = Asset,
  request_variant = Get,
  response = crate::client::AssetGot,
  response_variant = Got,
  error = crate::client::AssetNotFound,
  error_variant = NotFound,
)]
pub struct AssetGet {
  pub id: String,
  #[ts(type = "string")]
  pub request_id: Uuid,
}

/// Webapp hint to warm the asset cache for a set of ids so subsequent
/// `Get` calls hit cache. Fire-and-forget; webapps observe completion
/// via `Asset.Ready` events.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct AssetPreload {
  pub ids: Vec<String>,
}

/// Webapp-side asset operations.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum, BridgeDispatch)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::ClientToBridgeMsgData)]
pub enum ClientToBridgeAssetMsg {
  #[bridge_request]
  Get(AssetGet),
  #[bridge_command]
  Preload(AssetPreload),
}
