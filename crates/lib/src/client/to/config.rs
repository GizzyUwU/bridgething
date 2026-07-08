use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ConfigEntry;

/// Reply to `ConfigGet`.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct ConfigGetReply {
  pub key: String,
  /// `None` when the gateway has never set this key, as opposed to an empty string.
  pub value: Option<String>,
}

/// Reply to `ConfigList`.
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
/// Daemon -> webapp config replies and events. `Get` / `List` reply to
/// the matching requests. `Changed` broadcasts whenever the gateway
/// writes or deletes a value for the active webapp.
pub enum BridgeToClientConfigMsg {
  #[bridge_response]
  Get(ConfigGetReply),
  #[bridge_response]
  List(ConfigListReply),
  #[bridge_event]
  Changed(ConfigChanged),
}
