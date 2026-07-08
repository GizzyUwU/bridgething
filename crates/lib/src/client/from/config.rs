use bridgething_macros::{BridgeEnum, WireRequest};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Webapp request: read one config value for the currently active webapp,
/// as most recently set by the gateway. This surface is read-only; only
/// the gateway can write config.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[wire_request(
  direction = ClientToBridge,
  surface = Config,
  request_variant = Get,
  response = crate::client::ConfigGetReply,
  response_variant = Get,
)]
pub struct ConfigGet {
  pub key: String,
}

/// Marker request: webapp asks for every config entry the gateway has
/// set for the currently active webapp.
#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = ClientToBridge,
  surface = Config,
  request_variant = List,
  response = crate::client::ConfigListReply,
  response_variant = List,
)]
pub struct ConfigList;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::ClientToBridgeMsgData)]
/// Webapp -> daemon read-only config surface (`client.config`). Config
/// values are user-tunable settings the gateway pushes down for the
/// active webapp; webapps cannot write here. `Get` reads a single key,
/// `List` reads every set key. The daemon also broadcasts
/// `BridgeToClientConfigMsg::Changed` whenever the gateway writes a
/// value, so most webapps don't need to poll `Get`.
pub enum ClientToBridgeConfigMsg {
  #[bridge_request]
  Get(ConfigGet),
  #[bridge_request]
  List,
}
