use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{DocEntry, WebappError};

/// Reply to `DocGet`.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct DocGetReply {
  pub key: String,
  /// `None` when the key has never been written.
  pub value: Option<String>,
}

/// Reply to `DocList`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct DocListReply {
  pub entries: Vec<DocEntry>,
}

/// Ack for `DocSet` / `DocDelete`; echoes what's now stored.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct DocAck {
  pub key: String,
  pub value: Option<String>,
}

/// Broadcast when the COMPANION writes the active webapp's doc namespace.
/// Webapp-origin writes are not echoed back (the writer already holds the
/// ack).
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct DocChanged {
  pub key: String,
  /// `None` means the entry was deleted.
  pub value: Option<String>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::BridgeToClientMsgData)]
/// Daemon -> webapp doc replies and events. `Changed` broadcasts whenever
/// the companion writes or deletes a doc value for the active webapp.
pub enum BridgeToClientDocMsg {
  #[bridge_response]
  Get(DocGetReply),
  #[bridge_response]
  List(DocListReply),
  #[bridge_response]
  Ack(DocAck),
  /// domain-level error response (oversized value)
  #[bridge_response]
  Error(WebappError),
  #[bridge_event]
  Changed(DocChanged),
}
