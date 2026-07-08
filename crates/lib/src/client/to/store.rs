use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Reply to `KVGet`, `KVPut`, and `KVDelete` alike; the request that
/// produced it distinguishes which operation happened.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct StorageResponse {
  pub key: String,
  /// `None` on a `Get` miss or after a `Delete`; `Put` always echoes back `Some` of what it just wrote.
  pub value: Option<String>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::BridgeToClientMsgData)]
/// Daemon -> webapp KV storage replies. `Response` answers all three
/// `client.store` requests (`Get` / `Put` / `Delete`).
pub enum BridgeToClientStoreMsg {
  #[bridge_response]
  Response(StorageResponse),
}
