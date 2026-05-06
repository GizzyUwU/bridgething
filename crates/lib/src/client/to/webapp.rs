use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::WebappInfo;

#[serde_with::serde_as]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct WebappIconReply {
  #[serde_as(as = "serde_with::Bytes")]
  #[ts(type = "Uint8Array")]
  pub bytes: Vec<u8>,
  pub mime: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct WebappListReply {
  pub webapps: Vec<WebappInfo>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct WebappCurrentReply {
  #[ts(type = "string | null")]
  pub id: Option<Uuid>,
  pub name: Option<String>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct WebappActiveReply {
  #[ts(type = "string | null")]
  pub id: Option<Uuid>,
  pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct WebappInstalledReply {
  pub info: WebappInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub enum WebappError {
  /// The named webapp is not installed and not built-in.
  NotFound { name: String },
  /// Install was attempted with a name that is already installed.
  AlreadyInstalled { name: String },
  /// The supplied bundle is not a valid webapp (no index.html, missing
  /// metadata, archive corrupt, etc.).
  InvalidBundle { reason: String },
  /// Install failed for a reason orthogonal to the bundle (disk full,
  /// asset id not in cache, etc.).
  InstallFailed { reason: String },
  /// No installed webapp matches this id.
  WebappNotFound { id: String },
  /// The webapp's manifest doesn't declare an icon (or it's missing on disk).
  IconNotAvailable { id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct WebappErrorReply {
  pub error: WebappError,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct WebappActiveChanged {
  pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct WebappInstallProgress {
  pub name: String,
  pub percent: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct WebappUninstalled {
  pub name: String,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::BridgeToClientMsgData)]
pub enum BridgeToClientWebappMsg {
  #[bridge_response]
  ListReply(WebappListReply),
  #[bridge_response]
  CurrentReply(WebappCurrentReply),
  #[bridge_response]
  ActiveReply(WebappActiveReply),
  #[bridge_response]
  ErrorReply(WebappErrorReply),
  #[bridge_response]
  IconReply(WebappIconReply),
  #[bridge_event]
  ActiveChanged(WebappActiveChanged),
  #[bridge_event]
  WebappInstalled(WebappInstalledReply),
  #[bridge_event]
  InstallProgress(WebappInstallProgress),
  #[bridge_event]
  WebappUninstalled(WebappUninstalled),
}
