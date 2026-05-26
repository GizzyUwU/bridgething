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
  error = crate::WebappError,
  error_variant = WebappError,
)]
pub struct WebappSwitchTo {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub id: Uuid,
}

/// Companion-initiated chunked webapp install: opens or resumes a
/// streaming push of a zip bundle. Daemon responds with
/// `WebappInstallBeginAck { resume_from_offset }` (the byte offset the
/// next `WebappInstallChunk` should start at, 0 for fresh pushes) or a
/// `WebappError` variant (already-running install, mismatched size/sha
/// on conflicting in-flight install_id, etc).
///
/// `install_id` is the sha256 of the .zip, hex-encoded. Content-
/// addressed so resume across daemon restarts and retries-after-failure
/// both work without companion-side state to track. The terminal
/// outcome - `WebappInstalled(WebappInfo)` event on success or
/// `WebappInstallFailed { install_id, error }` event on failure -
/// arrives asynchronously after the last chunk lands; between the last
/// `WebappInstallChunk` ack and the terminal event the install is
/// implicitly in "installing" state.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = GatewayToBridge,
  surface = Webapp,
  request_variant = InstallBegin,
  response = crate::gateway::WebappInstallBeginAck,
  response_variant = InstallBeginAck,
  error = crate::WebappError,
  error_variant = WebappError,
)]
pub struct WebappInstallBegin {
  pub install_id: String,
  pub expected_sha256: String,
  pub expected_size: u32,
}

/// Streaming chunk of a webapp install upload opened by
/// `WebappInstallBegin`. `offset` must equal the daemon's current
/// `received` for the transfer (chunks are strictly in-order; the
/// companion learns the resume offset from `WebappInstallBeginAck`).
/// `last:true` triggers post-stream verify (size + sha256) followed by
/// extract + validate + install. Terminal outcome arrives as
/// `WebappInstalled` event or `WebappInstallFailed` event.
#[typeshare]
#[serde_with::serde_as]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct WebappInstallChunk {
  pub install_id: String,
  pub offset: u32,
  #[debug(skip)]
  #[serde_as(as = "serde_with::Bytes")]
  #[ts(type = "Uint8Array")]
  pub bytes: Vec<u8>,
  pub last: bool,
}

/// Drop the daemon-side partial for `install_id`. The chunked-transfer
/// subsystem also runs a 24h stale GC for partials that were never
/// abandoned, so this is an explicit cleanup, not a correctness gate.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct WebappInstallAbandon {
  pub install_id: String,
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
  error = crate::WebappError,
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
  error = crate::WebappError,
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
  error = crate::WebappError,
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
  error = crate::WebappError,
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
  error = crate::WebappError,
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
  error = crate::WebappError,
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
  /// request: open a chunked install upload; bridge replies with
  /// `InstallBeginAck { resume_from_offset }` or `WebappError`.
  #[bridge_request]
  InstallBegin(WebappInstallBegin),
  /// event: streaming chunk for an in-flight install upload. Companion
  /// emits on the Bulk lane; daemon writes to disk via ChunkedTransfer.
  /// Terminal outcome arrives as `WebappInstalled` / `WebappInstallFailed`
  /// event after `last:true`.
  #[bridge_event]
  InstallChunk(WebappInstallChunk),
  /// command: drop the daemon-side partial for `install_id`.
  #[bridge_command]
  InstallAbandon(WebappInstallAbandon),
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
