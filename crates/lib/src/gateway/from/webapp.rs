use bridgething_macros::{BridgeEnum, GatewayRequest};
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

/// Marker struct for the `List` request — pairs with `BridgeToGatewayWebappMsg::Webapps`.
#[derive(Debug, Clone, Copy, Default, GatewayRequest)]
#[gateway_request(
  surface = Webapp,
  request_variant = List,
  response = crate::gateway::WebappList,
  response_variant = Webapps,
)]
pub struct ListWebapps;

/// Marker struct for the `GetActive` request — pairs with `BridgeToGatewayWebappMsg::Active`.
#[derive(Debug, Clone, Copy, Default, GatewayRequest)]
#[gateway_request(
  surface = Webapp,
  request_variant = GetActive,
  response = crate::gateway::WebappActive,
  response_variant = Active,
)]
pub struct GetActiveWebapp;

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, GatewayRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[gateway_request(
  surface = Webapp,
  request_variant = SwitchTo,
  response = crate::gateway::WebappActive,
  response_variant = Switched,
  error = crate::gateway::WebappError,
  error_variant = WebappError,
)]
pub struct WebappSwitchTo {
  pub name: String,
}

#[typeshare]
#[serde_with::serde_as]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, GatewayRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[gateway_request(
  surface = Webapp,
  request_variant = Install,
  response = crate::WebappInfo,
  response_variant = Installed,
  error = crate::gateway::WebappError,
  error_variant = WebappError,
)]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, GatewayRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[gateway_request(
  surface = Webapp,
  request_variant = Uninstall,
  response = crate::gateway::WebappActive,
  response_variant = Uninstalled,
  error = crate::gateway::WebappError,
  error_variant = WebappError,
)]
pub struct WebappUninstall {
  pub name: String,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::GatewayToBridgeMsgData)]
pub enum GatewayToBridgeWebappMsg {
  /// request: bridge replies with `Webapps`
  #[bridge_request]
  List,
  /// request: bridge replies with `Active`
  #[bridge_request]
  GetActive,
  /// command: switch the kiosk to the named webapp; bridge replies with `Switched`
  #[bridge_request]
  SwitchTo(WebappSwitchTo),
  /// command: extract the supplied zip into the installed root under `name`;
  /// bridge replies with `Installed`
  #[bridge_request]
  Install(WebappInstall),
  /// command: remove the named installed webapp; bridge replies with `Uninstalled`
  /// (built-ins cannot be removed and surface as `WebappError::CannotUninstallBuiltin`)
  #[bridge_request]
  Uninstall(WebappUninstall),
}
