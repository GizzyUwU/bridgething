use bridgething_macros::{BridgeEnum, WireRequest};
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

/// Marker struct for the `List` request - pairs with `BridgeToGatewayWebappMsg::Webapps`.
#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = GatewayToBridge,
  surface = Webapp,
  request_variant = List,
  response = crate::gateway::WebappList,
  response_variant = Webapps,
)]
pub struct ListWebapps;

/// Marker struct for the `GetActive` request - pairs with `BridgeToGatewayWebappMsg::Active`.
#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = GatewayToBridge,
  surface = Webapp,
  request_variant = GetActive,
  response = crate::gateway::WebappActive,
  response_variant = Active,
)]
pub struct GetActiveWebapp;

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = GatewayToBridge,
  surface = Webapp,
  request_variant = SwitchTo,
  response = crate::gateway::WebappActive,
  response_variant = Switched,
  error = crate::gateway::WebappError,
  error_variant = WebappError,
)]
pub struct WebappSwitchTo {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub id: Uuid,
}

#[typeshare]
#[serde_with::serde_as]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = GatewayToBridge,
  surface = Webapp,
  request_variant = Install,
  response = crate::WebappInfo,
  response_variant = Installed,
  error = crate::gateway::WebappError,
  error_variant = WebappError,
)]
pub struct WebappInstall {
  /// zip archive whose top-level entries become the bundle contents.
  /// Must include an `index.html` and a valid `manifest.json` at the
  /// archive root. The bundle's identity comes from the manifest's id.
  #[debug(skip)]
  #[serde_as(as = "serde_with::Bytes")]
  #[ts(type = "Uint8Array")]
  pub archive: Vec<u8>,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = GatewayToBridge,
  surface = Webapp,
  request_variant = Uninstall,
  response = crate::gateway::WebappActive,
  response_variant = Uninstalled,
  error = crate::gateway::WebappError,
  error_variant = WebappError,
)]
pub struct WebappUninstall {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub id: Uuid,
}

/// Icon byte fetch. Daemon reads the icon declared by the manifest from
/// the bundle directory and returns the bytes + mime.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = GatewayToBridge,
  surface = Webapp,
  request_variant = Icon,
  response = crate::gateway::WebappIconReply,
  response_variant = Icon,
  error = crate::gateway::WebappError,
  error_variant = WebappError,
)]
pub struct WebappIcon {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub id: Uuid,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = GatewayToBridge,
  surface = Webapp,
  request_variant = ConfigGet,
  response = crate::gateway::WebappConfigGetReply,
  response_variant = ConfigGet,
  error = crate::gateway::WebappError,
  error_variant = WebappError,
)]
pub struct WebappConfigGet {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub id: Uuid,
  pub key: String,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = GatewayToBridge,
  surface = Webapp,
  request_variant = ConfigList,
  response = crate::gateway::WebappConfigListReply,
  response_variant = ConfigList,
  error = crate::gateway::WebappError,
  error_variant = WebappError,
)]
pub struct WebappConfigList {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub id: Uuid,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = GatewayToBridge,
  surface = Webapp,
  request_variant = ConfigSet,
  response = crate::gateway::WebappConfigAck,
  response_variant = ConfigAck,
  error = crate::gateway::WebappError,
  error_variant = WebappError,
)]
pub struct WebappConfigSet {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub id: Uuid,
  pub key: String,
  pub value: String,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = GatewayToBridge,
  surface = Webapp,
  request_variant = ConfigDelete,
  response = crate::gateway::WebappConfigAck,
  response_variant = ConfigAck,
  error = crate::gateway::WebappError,
  error_variant = WebappError,
)]
pub struct WebappConfigDelete {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub id: Uuid,
  pub key: String,
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
  /// request: bridge replies with `Icon`
  #[bridge_request]
  Icon(WebappIcon),
  #[bridge_request]
  ConfigGet(WebappConfigGet),
  #[bridge_request]
  ConfigList(WebappConfigList),
  #[bridge_request]
  ConfigSet(WebappConfigSet),
  #[bridge_request]
  ConfigDelete(WebappConfigDelete),
}
