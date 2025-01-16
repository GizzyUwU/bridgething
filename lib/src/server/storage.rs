use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "action", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "server.ts")]
pub enum ServerStorageEvent {
  Response { key: String, value: Option<String> },
}
