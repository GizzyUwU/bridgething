use bridgething_macros::{BridgeEnum, WireRequest};
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = GatewayToBridge,
  surface = Webapp,
  request_variant = List,
  response = crate::gateway::WebappList,
  response_variant = Webapps,
)]
pub struct ListWebapps;

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

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum WebappResourceKind {
  Icon,
  Settings,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = GatewayToBridge,
  surface = Webapp,
  request_variant = Resource,
  response = crate::gateway::WebappResourceReply,
  response_variant = Resource,
  error = crate::WebappError,
  error_variant = WebappError,
)]
pub struct WebappResource {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub id: Uuid,
  pub kind: WebappResourceKind,
  pub have: Option<String>,
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = GatewayToBridge,
  surface = Webapp,
  request_variant = DocGet,
  response = crate::gateway::WebappDocGetReply,
  response_variant = DocGet,
  error = crate::WebappError,
  error_variant = WebappError,
)]
pub struct WebappDocGet {
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
  request_variant = DocList,
  response = crate::gateway::WebappDocListReply,
  response_variant = DocList,
  error = crate::WebappError,
  error_variant = WebappError,
)]
pub struct WebappDocList {
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
  request_variant = DocSet,
  response = crate::gateway::WebappDocAck,
  response_variant = DocAck,
  error = crate::WebappError,
  error_variant = WebappError,
)]
pub struct WebappDocSet {
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
  request_variant = DocDelete,
  response = crate::gateway::WebappDocAck,
  response_variant = DocAck,
  error = crate::WebappError,
  error_variant = WebappError,
)]
pub struct WebappDocDelete {
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
  #[bridge_request]
  List,
  #[bridge_request]
  GetActive,
  #[bridge_request]
  SwitchTo(WebappSwitchTo),
  #[bridge_request]
  Uninstall(WebappUninstall),
  #[bridge_request]
  Resource(WebappResource),
  #[bridge_request]
  ConfigGet(WebappConfigGet),
  #[bridge_request]
  ConfigList(WebappConfigList),
  #[bridge_request]
  ConfigSet(WebappConfigSet),
  #[bridge_request]
  ConfigDelete(WebappConfigDelete),
  #[bridge_request]
  DocGet(WebappDocGet),
  #[bridge_request]
  DocList(WebappDocList),
  #[bridge_request]
  DocSet(WebappDocSet),
  #[bridge_request]
  DocDelete(WebappDocDelete),
}
