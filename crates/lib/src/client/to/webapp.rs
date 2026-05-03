use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::WebappInfo;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
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
  pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct WebappActiveReply {
  pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
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
  UninstalledReply(WebappActiveReply),
  #[bridge_response]
  InstalledReply(WebappInstalledReply),
  #[bridge_response]
  ErrorReply(WebappErrorReply),
  #[bridge_event]
  ActiveChanged(WebappActiveChanged),
  #[bridge_event]
  WebappInstalled(WebappInstalledReply),
  #[bridge_event]
  InstallProgress(WebappInstallProgress),
  #[bridge_event]
  WebappUninstalled(WebappUninstalled),
}
