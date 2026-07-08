use bridgething_macros::{BridgeDispatch, BridgeEnum, WireRequest};
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// Webapp asks for the list of webapps visible to clients: built-in and installed, excluding
/// `Launcher`-role bundles.
#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = ClientToBridge,
  surface = Webapp,
  request_variant = List,
  response = crate::client::WebappListReply,
  response_variant = ListReply,
)]
pub struct WebappList;

/// Webapp asks which webapp (if any) is currently active in the kiosk.
#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = ClientToBridge,
  surface = Webapp,
  request_variant = Current,
  response = crate::client::WebappCurrentReply,
  response_variant = CurrentReply,
)]
pub struct WebappCurrent;

/// Payload for the `activate` request: switch the kiosk to the given webapp. The kiosk runs
/// exactly one webapp at a time, so the daemon navigates it away from whatever was previously
/// active.
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
  /// Id of an installed webapp, from `webapp.list`.
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
  /// Id of an installed webapp, from `webapp.list`.
  #[ts(type = "string")]
  pub id: Uuid,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum, BridgeDispatch)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::ClientToBridgeMsgData)]
/// Webapp -> daemon webapp-management surface: enumerate installed webapps, inspect which one
/// is active, switch the kiosk to a different webapp, and fetch icon bytes. All four verbs are
/// request/reply.
pub enum ClientToBridgeWebappMsg {
  #[bridge_request]
  List,
  #[bridge_request]
  Current,
  #[bridge_request]
  Activate(WebappActivate),
  #[bridge_request]
  Icon(WebappIcon),
}
