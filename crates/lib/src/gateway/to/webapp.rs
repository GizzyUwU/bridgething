use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

use crate::WebappInfo;

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
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
  pub name: String,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum BridgeToGatewayWebappMsg {
  /// response to List
  Webapps(WebappList),
  /// response to GetActive, and event broadcast on switch
  Active(WebappActive),
  /// response to SwitchTo indicating the new active app
  Switched(WebappActive),
  /// response to Install indicating the freshly installed app's metadata
  Installed(WebappInfo),
  /// response to Uninstall carrying the active app after the uninstall settled
  Uninstalled(WebappActive),
  /// domain-level error response for any webapp op (e.g. UnknownWebapp,
  /// CannotUninstallBuiltin)
  WebappError(WebappError),
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum WebappError {
  /// The named webapp is not installed and not built-in.
  UnknownWebapp { name: String },
  /// Built-in webapps cannot be uninstalled.
  CannotUninstallBuiltin { name: String },
  /// The install archive could not be applied (corrupt zip, missing index.html, etc).
  InstallFailed { reason: String },
}
