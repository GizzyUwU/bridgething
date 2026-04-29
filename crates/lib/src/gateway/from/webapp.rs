use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct WebappSwitchTo {
  pub name: String,
}

#[typeshare]
#[serde_with::serde_as]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct WebappInstall {
  pub name: String,
  /// zip archive whose top-level entries become the bundle contents.
  /// Must include an `index.html` at the archive root.
  #[debug(skip)]
  #[serde_as(as = "serde_with::Bytes")]
  #[ts(type = "Uint8Array")]
  pub archive: Vec<u8>,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct WebappUninstall {
  pub name: String,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum GatewayToBridgeWebappMsg {
  /// request: bridge replies with the full list of installed + built-in webapps
  List,
  /// request: bridge replies with the active webapp's name
  GetActive,
  /// command: switch the kiosk to the named webapp; bridge replies with `Switched`
  SwitchTo(WebappSwitchTo),
  /// command: extract the supplied zip into the installed root under `name`
  Install(WebappInstall),
  /// command: remove the named installed webapp (built-ins cannot be removed)
  Uninstall(WebappUninstall),
}
