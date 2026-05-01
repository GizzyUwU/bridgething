use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

use crate::{
  WebappInfo,
  gateway::{
    BridgeToGatewayMsgData, BridgeToGatewayWebappMsg, GatewayToBridgeMsgData, WebappActive, WebappError, WebappList,
  },
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
  response: WebappList,
  encode_request:
    _r => GatewayToBridgeMsgData::Webapp(GatewayToBridgeWebappMsg::List),
  extract_response:
    BridgeToGatewayMsgData::Webapp(BridgeToGatewayWebappMsg::Webapps(v)) => v,
  encode_response:
    v => BridgeToGatewayMsgData::Webapp(BridgeToGatewayWebappMsg::Webapps(v)),
}

impl_gateway_request! {
  request: GetActiveWebapp,
  response: WebappActive,
  encode_request:
    _r => GatewayToBridgeMsgData::Webapp(GatewayToBridgeWebappMsg::GetActive),
  extract_response:
    BridgeToGatewayMsgData::Webapp(BridgeToGatewayWebappMsg::Active(v)) => v,
  encode_response:
    v => BridgeToGatewayMsgData::Webapp(BridgeToGatewayWebappMsg::Active(v)),
}

impl_gateway_request! {
  request: WebappSwitchTo,
  response: WebappActive,
  error: WebappError,
  encode_request:
    r => GatewayToBridgeMsgData::Webapp(GatewayToBridgeWebappMsg::SwitchTo(r)),
  extract_response:
    BridgeToGatewayMsgData::Webapp(BridgeToGatewayWebappMsg::Switched(v)) => v,
  encode_response:
    v => BridgeToGatewayMsgData::Webapp(BridgeToGatewayWebappMsg::Switched(v)),
  extract_error:
    BridgeToGatewayMsgData::Webapp(BridgeToGatewayWebappMsg::WebappError(e)) => e,
  encode_error:
    e => BridgeToGatewayMsgData::Webapp(BridgeToGatewayWebappMsg::WebappError(e)),
}

impl_gateway_request! {
  request: WebappInstall,
  response: WebappInfo,
  error: WebappError,
  encode_request:
    r => GatewayToBridgeMsgData::Webapp(GatewayToBridgeWebappMsg::Install(r)),
  extract_response:
    BridgeToGatewayMsgData::Webapp(BridgeToGatewayWebappMsg::Installed(v)) => v,
  encode_response:
    v => BridgeToGatewayMsgData::Webapp(BridgeToGatewayWebappMsg::Installed(v)),
  extract_error:
    BridgeToGatewayMsgData::Webapp(BridgeToGatewayWebappMsg::WebappError(e)) => e,
  encode_error:
    e => BridgeToGatewayMsgData::Webapp(BridgeToGatewayWebappMsg::WebappError(e)),
}

impl_gateway_request! {
  request: WebappUninstall,
  response: WebappActive,
  error: WebappError,
  encode_request:
    r => GatewayToBridgeMsgData::Webapp(GatewayToBridgeWebappMsg::Uninstall(r)),
  extract_response:
    BridgeToGatewayMsgData::Webapp(BridgeToGatewayWebappMsg::Uninstalled(v)) => v,
  encode_response:
    v => BridgeToGatewayMsgData::Webapp(BridgeToGatewayWebappMsg::Uninstalled(v)),
  extract_error:
    BridgeToGatewayMsgData::Webapp(BridgeToGatewayWebappMsg::WebappError(e)) => e,
  encode_error:
    e => BridgeToGatewayMsgData::Webapp(BridgeToGatewayWebappMsg::WebappError(e)),
}
