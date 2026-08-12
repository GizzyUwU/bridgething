use bytes::Bytes;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct TunnelData {
  #[ts(type = "string")]
  pub tunnel_id: Uuid,
  #[ts(type = "Uint8Array")]
  pub bytes: Bytes,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct TunnelAck {
  #[ts(type = "string")]
  pub tunnel_id: Uuid,
  pub consumed: u32,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct TunnelClosed {
  #[ts(type = "string")]
  pub tunnel_id: Uuid,
  pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum TunnelError {
  ConnectFailed { reason: String },
  PermissionDenied,
  Unavailable,
}
