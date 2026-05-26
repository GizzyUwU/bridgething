use bridgething_macros::{BridgeDispatch, BridgeEnum, WireRequest};
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

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
  error = crate::WebappError,
  error_variant = WebappError,
)]
pub struct WebappActivate {
  #[ts(type = "string")]
  pub id: Uuid,
}

/// Fetch the icon bytes for an installed webapp. Returns the raw bytes
/// declared by the manifest's `icon` field.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[wire_request(
  direction = ClientToBridge,
  surface = Webapp,
  request_variant = Icon,
  response = crate::client::WebappIconReply,
  response_variant = IconReply,
  error = crate::WebappError,
  error_variant = WebappError,
)]
pub struct WebappIcon {
  #[ts(type = "string")]
  pub id: Uuid,
}

/// Webapp-initiated chunked install. `install_id` is the sha256 hex of
/// the zip. The terminal `WebappInstalled` / `WebappInstallFailed`
/// events broadcast to both gateway and webapp peers regardless of
/// which surface initiated the install.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[wire_request(
  direction = ClientToBridge,
  surface = Webapp,
  request_variant = InstallBegin,
  response = crate::client::WebappInstallBeginAck,
  response_variant = InstallBeginAck,
  error = crate::WebappError,
  error_variant = WebappError,
)]
pub struct WebappInstallBegin {
  pub install_id: String,
  pub expected_sha256: String,
  pub expected_size: u32,
}

#[typeshare]
#[serde_with::serde_as]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct WebappInstallChunk {
  pub install_id: String,
  pub offset: u32,
  #[debug(skip)]
  #[serde_as(as = "serde_with::Bytes")]
  #[ts(type = "Uint8Array")]
  pub bytes: Vec<u8>,
  pub last: bool,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct WebappInstallAbandon {
  pub install_id: String,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum, BridgeDispatch)]
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
  Icon(WebappIcon),
  /// request: open a chunked install upload; bridge replies with
  /// `InstallBeginAck { resume_from_offset }` or `WebappError`.
  #[bridge_request]
  InstallBegin(WebappInstallBegin),
  /// command: streaming chunk for an in-flight install upload. The
  /// terminal `WebappInstalled` / `WebappInstallFailed` event arrives
  /// after `last:true`.
  #[bridge_command]
  InstallChunk(WebappInstallChunk),
  /// command: drop the daemon-side partial for `install_id`.
  #[bridge_command]
  InstallAbandon(WebappInstallAbandon),
}
