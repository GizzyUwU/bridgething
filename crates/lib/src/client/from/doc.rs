use bridgething_macros::{BridgeEnum, WireRequest};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Webapp request: read one doc value for the currently active webapp.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[wire_request(
  direction = ClientToBridge,
  surface = Doc,
  request_variant = Get,
  response = crate::client::DocGetReply,
  response_variant = Get,
)]
pub struct DocGet {
  pub key: String,
}

/// Marker request: read every doc entry for the currently active webapp.
#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = ClientToBridge,
  surface = Doc,
  request_variant = List,
  response = crate::client::DocListReply,
  response_variant = List,
)]
pub struct DocList;

/// Webapp request: write a doc value. Last write wins against companion
/// writes on the same key; the companion hears the change as a gateway
/// `webapp.docChanged` event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[wire_request(
  direction = ClientToBridge,
  surface = Doc,
  request_variant = Set,
  response = crate::client::DocAck,
  response_variant = Ack,
  error = crate::WebappError,
  error_variant = Error,
)]
pub struct DocSet {
  pub key: String,
  pub value: String,
}

/// Webapp request: delete the doc entry at `key`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[wire_request(
  direction = ClientToBridge,
  surface = Doc,
  request_variant = Delete,
  response = crate::client::DocAck,
  response_variant = Ack,
)]
pub struct DocDelete {
  pub key: String,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::ClientToBridgeMsgData)]
/// Webapp -> daemon doc surface (`client.doc`). The doc namespace is
/// shared structured state per webapp, writable from BOTH the webapp
/// and the companion (which authors it from the app's settings page).
/// Last write wins; the daemon broadcasts `BridgeToClientDocMsg::Changed`
/// on companion-origin writes so the running app applies them live.
pub enum ClientToBridgeDocMsg {
  #[bridge_request]
  Get(DocGet),
  #[bridge_request]
  List,
  #[bridge_request]
  Set(DocSet),
  #[bridge_request]
  Delete(DocDelete),
}
