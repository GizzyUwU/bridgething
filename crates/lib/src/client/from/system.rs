use bridgething_macros::{BridgeEnum, WireRequest};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{LogLevel, LogSource};

/// Marker request: webapp asks the bridge for its `BridgeThingMeta`.
/// Pairs with `BridgeToClientSystemMsg::Version`.
#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = ClientToBridge,
  surface = System,
  request_variant = VersionRequest,
  response = crate::BridgeThingMeta,
  response_variant = Version,
  boxed_response,
)]
pub struct RequestVersion;

#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = ClientToBridge,
  surface = System,
  request_variant = DiagnosticsGet,
  response = crate::client::DiagnosticsReply,
  response_variant = DiagnosticsReply,
)]
pub struct DiagnosticsGet;

/// Pull a one-shot batch of recent log entries. Filtering happens
/// daemon-side before any wire allocation.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[wire_request(
  direction = ClientToBridge,
  surface = System,
  request_variant = LogsTail,
  response = crate::client::LogsTailReply,
  response_variant = LogsTailReply,
)]
pub struct LogsTail {
  pub source: LogSource,
  pub levels: Vec<LogLevel>,
  pub filter: Option<String>,
  pub max_lines: u32,
}

/// Open a streaming subscription. Daemon returns a `LogsSubscribeReply`
/// with an opaque token; webapp passes the token to `LogsUnsubscribe`
/// to release. Subscriptions are scoped to the WS connection - the
/// daemon auto-releases on disconnect.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[wire_request(
  direction = ClientToBridge,
  surface = System,
  request_variant = LogsSubscribe,
  response = crate::client::LogsSubscribeReply,
  response_variant = LogsSubscribeReply,
)]
pub struct LogsSubscribe {
  pub source: LogSource,
  pub levels: Vec<LogLevel>,
  pub filter: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct LogsUnsubscribe {
  pub token: String,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::ClientToBridgeMsgData)]
pub enum ClientToBridgeSystemMsg {
  #[bridge_request]
  VersionRequest,
  #[bridge_request]
  DiagnosticsGet,
  #[bridge_request]
  LogsTail(LogsTail),
  #[bridge_request]
  LogsSubscribe(LogsSubscribe),
  #[bridge_command]
  LogsUnsubscribe(LogsUnsubscribe),
  #[bridge_command]
  Reboot,
  #[bridge_command]
  PowerOff,
  #[bridge_command]
  FactoryReset,
}
