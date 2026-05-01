use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

#[serde_with::serde_as]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "server.ts")]
pub struct AssetGot {
  #[ts(type = "string")]
  pub request_id: Uuid,
  pub id: String,
  #[debug(skip)]
  #[serde_as(as = "serde_with::Bytes")]
  #[ts(type = "Uint8Array")]
  pub bytes: Vec<u8>,
  pub mime: Option<String>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "server.ts")]
pub struct AssetNotFound {
  #[ts(type = "string")]
  pub request_id: Uuid,
  pub id: String,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "server.ts")]
pub struct AssetReady {
  pub id: String,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "server.ts")]
pub struct AssetCleared {
  pub id: String,
}

/// Daemon-side asset events. `Got` and `NotFound` resolve a webapp `Get`
/// (correlated by request_id). `Ready` broadcasts to all connected
/// webapps whenever the cache gains an asset, regardless of source
/// (companion push, iAP2 FileTransfer, request fulfilment, lazy disk
/// load). `Cleared` broadcasts on every eviction path - LRU pressure,
/// TTL expiry, companion-issued Clear, daemon shutdown - so SDK
/// consumers can drop Blob URLs and refetch as needed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "server.ts")]
pub enum ServerAssetEvent {
  Got(AssetGot),
  NotFound(AssetNotFound),
  Ready(AssetReady),
  Cleared(AssetCleared),
}
