use bridgething_macros::{BridgeEnum, WireRequest};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = ClientToBridge,
  surface = Webapp,
  request_variant = List,
  response = crate::client::WebappListReply,
  response_variant = ListReply,
)]
pub struct WebappList;

#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = ClientToBridge,
  surface = Webapp,
  request_variant = Current,
  response = crate::client::WebappCurrentReply,
  response_variant = CurrentReply,
)]
pub struct WebappCurrent;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[wire_request(
  direction = ClientToBridge,
  surface = Webapp,
  request_variant = Activate,
  response = crate::client::WebappActiveReply,
  response_variant = ActiveReply,
  error = crate::client::WebappErrorReply,
  error_variant = ErrorReply,
)]
pub struct WebappActivate {
  pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[wire_request(
  direction = ClientToBridge,
  surface = Webapp,
  request_variant = Uninstall,
  response = crate::client::WebappActiveReply,
  response_variant = UninstalledReply,
  error = crate::client::WebappErrorReply,
  error_variant = ErrorReply,
)]
pub struct WebappUninstall {
  pub name: String,
}

/// Install a webapp from a previously-pushed `.zip` archive in the
/// AssetCache. The webapp surface validates structure, lays out the
/// bundle, and emits `onWebappInstalled` on success.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[wire_request(
  direction = ClientToBridge,
  surface = Webapp,
  request_variant = Install,
  response = crate::client::WebappInstalledReply,
  response_variant = InstalledReply,
  error = crate::client::WebappErrorReply,
  error_variant = ErrorReply,
)]
pub struct WebappInstall {
  pub archive_asset_id: String,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::ClientToBridgeMsgData)]
pub enum ClientToBridgeWebappMsg {
  #[bridge_request]
  List,
  #[bridge_request]
  Current,
  #[bridge_request]
  Activate(WebappActivate),
  #[bridge_request]
  Uninstall(WebappUninstall),
  #[bridge_request]
  Install(WebappInstall),
}
