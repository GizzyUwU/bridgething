use bridgething_macros::BridgeEnum;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

use crate::{ConfigEntry, WebappInfo};

#[typeshare]
#[serde_with::serde_as]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct WebappIconReply {
  #[debug(skip)]
  #[serde_as(as = "serde_with::Bytes")]
  #[ts(type = "Uint8Array")]
  pub bytes: Vec<u8>,
  pub mime: Option<String>,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct WebappConfigGetReply {
  pub key: String,
  pub value: Option<String>,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct WebappConfigListReply {
  pub entries: Vec<ConfigEntry>,
}

/// Ack for WebappConfigSet / WebappConfigDelete. The `value` field
/// echoes what's now stored after the write (None for delete).
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct WebappConfigAck {
  pub key: String,
  pub value: Option<String>,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct WebappList {
  pub webapps: Vec<WebappInfo>,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct WebappActive {
  #[ts(type = "string | null")]
  #[typeshare(serialized_as = "Option<Vec<u8>>")]
  pub id: Option<Uuid>,
  pub name: Option<String>,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::BridgeToGatewayMsgData)]
pub enum BridgeToGatewayWebappMsg {
  /// response to List
  #[bridge_response]
  Webapps(WebappList),
  /// response to GetActive, and event broadcast on switch
  #[bridge_response]
  Active(WebappActive),
  /// response to SwitchTo indicating the new active app
  #[bridge_response]
  Switched(WebappActive),
  /// response to Install indicating the freshly installed app's metadata
  #[bridge_response]
  Installed(WebappInfo),
  /// response to Uninstall carrying the active app after the uninstall settled
  #[bridge_response]
  Uninstalled(WebappActive),
  /// domain-level error response for any webapp op (e.g. UnknownWebapp,
  /// CannotUninstallBuiltin)
  #[bridge_response]
  WebappError(WebappError),
  #[bridge_response]
  Icon(WebappIconReply),
  #[bridge_response]
  ConfigGet(WebappConfigGetReply),
  #[bridge_response]
  ConfigList(WebappConfigListReply),
  #[bridge_response]
  ConfigAck(WebappConfigAck),
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum WebappError {
  /// No installed webapp matches this id.
  WebappNotFound { id: String },
  /// Built-in webapps cannot be uninstalled.
  CannotUninstallBuiltin { id: String },
  /// The install archive could not be applied (corrupt zip, missing
  /// index.html, manifest validation failed, etc).
  InstallFailed { reason: String },
  /// The webapp's manifest doesn't declare an icon (or it's missing on disk).
  IconNotAvailable { id: String },
  /// Config key is not declared in the webapp's manifest schema.
  UnknownConfigKey { key: String },
  /// Value failed schema validation (out of range, regex mismatch, not in enum).
  InvalidConfigValue { key: String, reason: String },
}
