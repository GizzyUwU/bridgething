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
  /// Opaque asset id, e.g. `iap2/art/<persistent-hex>/<n>` for iAP2 art or a
  /// companion-defined shape like `spotify/img/<id>`.
  pub id: String,
  /// Correlates the `Got` or `NotFound` reply back to this request.
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
  /// Ids to prefetch, capped at 64 per call. Ids already cached and ids
  /// under the `iap2/art/` prefix are skipped, since iAP2 art only ever
  /// arrives via a push from the phone, not a pull request.
  pub ids: Vec<String>,
}

/// Webapp-side asset operations: `get` fetches bytes by id, `preload` warms
/// the cache ahead of time so a later `get` resolves instantly.
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
