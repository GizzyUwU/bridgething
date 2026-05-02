use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

use crate::{
  WebappInfo,
  gateway::{WebappActive, WebappError, WebappList},
  impl_gateway_request,
};

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
  /// request: bridge replies with `Webapps`
  List,
  /// request: bridge replies with `Active`
  GetActive,
  /// command: switch the kiosk to the named webapp; bridge replies with `Switched`
  SwitchTo(WebappSwitchTo),
  /// command: extract the supplied zip into the installed root under `name`;
  /// bridge replies with `Installed`
  Install(WebappInstall),
  /// command: remove the named installed webapp; bridge replies with `Uninstalled`
  /// (built-ins cannot be removed and surface as `WebappError::CannotUninstallBuiltin`)
  Uninstall(WebappUninstall),
}

/// Marker struct for the `List` request — pairs with `BridgeToGatewayWebappMsg::Webapps`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ListWebapps;

/// Marker struct for the `GetActive` request — pairs with `BridgeToGatewayWebappMsg::Active`.
#[derive(Debug, Clone, Copy, Default)]
pub struct GetActiveWebapp;

impl_gateway_request! {
  request: ListWebapps,
  surface: Webapp,
  request_variant: List,
  response: WebappList,
  response_variant: Webapps(_),
}

impl_gateway_request! {
  request: GetActiveWebapp,
  surface: Webapp,
  request_variant: GetActive,
  response: WebappActive,
  response_variant: Active(_),
}

impl_gateway_request! {
  request: WebappSwitchTo,
  surface: Webapp,
  request_variant: SwitchTo(_),
  response: WebappActive,
  response_variant: Switched(_),
  error: WebappError,
  error_variant: WebappError(_),
}

impl_gateway_request! {
  request: WebappInstall,
  surface: Webapp,
  request_variant: Install(_),
  response: WebappInfo,
  response_variant: Installed(_),
  error: WebappError,
  error_variant: WebappError(_),
}

impl_gateway_request! {
  request: WebappUninstall,
  surface: Webapp,
  request_variant: Uninstall(_),
  response: WebappActive,
  response_variant: Uninstalled(_),
  error: WebappError,
  error_variant: WebappError(_),
}
