use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ConfigEntry;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct ConfigGetReply {
  pub key: String,
  pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct ConfigListReply {
  pub entries: Vec<ConfigEntry>,
}

/// Broadcast when the gateway writes a new value for the active webapp.
/// `value: None` means the entry was deleted; consumers should fall back
/// to whatever default they declared.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct ConfigChanged {
  pub key: String,
  pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::BridgeToClientMsgData)]
pub enum BridgeToClientConfigMsg {
  #[bridge_response]
  Get(ConfigGetReply),
  #[bridge_response]
  List(ConfigListReply),
  #[bridge_event]
  Changed(ConfigChanged),
}
