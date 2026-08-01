use bytes::Bytes;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct TunnelData {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub tunnel_id: Uuid,
  #[ts(type = "Uint8Array")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub bytes: Bytes,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct TunnelAck {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub tunnel_id: Uuid,
  pub consumed: u32,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct TunnelClosed {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub tunnel_id: Uuid,
  pub reason: Option<String>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum TunnelError {
  ConnectFailed { reason: String },
  PermissionDenied,
  Unavailable,
}
