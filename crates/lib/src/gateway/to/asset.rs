use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct AssetRequest {
  pub id: String,
  #[ts(type = "Uint8Array")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub request_id: Uuid,
}

/// Bridge-side asset operations. Daemon emits `Request` when a webapp asks
/// for an asset id that isn't in cache and a companion is connected; the
/// companion fulfils via `GatewayToBridgeAssetMsg::Push` carrying the same
/// id. v1 does not support bridge-initiated `Clear` - companions own the
/// retention lifecycle of anything they pushed.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum BridgeToGatewayAssetMsg {
  Request(AssetRequest),
}
