use bridgething_macros::{BridgeDispatch, BridgeEnum, WireRequest};
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

/// Marker request: webapp asks for a one-shot `Diagnostics` snapshot
/// (disk/memory usage, uptime, SoC temp, load average, versions).
/// Pairs with `BridgeToClientSystemMsg::DiagnosticsReply`.
#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = ClientToBridge,
  surface = System,
  request_variant = DiagnosticsGet,
  response = crate::client::DiagnosticsReply,
  response_variant = DiagnosticsReply,
)]
pub struct DiagnosticsGet;

/// Pull a one-shot batch of recent log entries. The source, levels, and
/// filter narrow the returned entries.
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
  /// Reserved for selecting the log source; the daemon serves its own tracing stream regardless.
  pub source: LogSource,
  /// Allow-list of levels to include; an empty vector matches every level.
  pub levels: Vec<LogLevel>,
  /// Case-sensitive substring match against `target` or `message`; `None` disables filtering.
  pub filter: Option<String>,
  /// Caps how many of the most recent matching entries return, in chronological order.
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
  /// Reserved for selecting the log source; the daemon serves its own tracing stream regardless.
  pub source: LogSource,
  /// Allow-list of levels to include; an empty vector matches every level.
  pub levels: Vec<LogLevel>,
  /// Case-sensitive substring match against `target` or `message`; `None` disables filtering.
  pub filter: Option<String>,
}

/// Release a log stream opened by `LogsSubscribe`. Fire-and-forget: unknown
/// or malformed tokens are silently ignored rather than surfacing an error.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct LogsUnsubscribe {
  pub token: String,
}

/// Webapp read of the device nickname. Read-only on this surface: only
/// the gateway-side `setNickname` can mutate. Webapps listen for
/// `DeviceNicknameChanged` events to track updates without polling.
#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = ClientToBridge,
  surface = System,
  request_variant = DeviceGetNickname,
  response = crate::client::DeviceNicknameReply,
  response_variant = DeviceNickname,
)]
pub struct DeviceGetNickname;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum, BridgeDispatch)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::ClientToBridgeMsgData)]
/// Webapp -> daemon system control surface. `VersionRequest` fetches the
/// daemon's `BridgeThingMeta` identity block; `DiagnosticsGet` fetches a
/// one-shot health snapshot; `LogsTail` pulls a batch of recent log
/// entries; `LogsSubscribe` / `LogsUnsubscribe` open and close a live log
/// stream; `Reboot` / `PowerOff` restart or shut down the device;
/// `FactoryReset` wipes daemon state (config, store, paired devices) and
/// reboots; `DeviceGetNickname` reads the current device nickname.
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
  #[bridge_request]
  DeviceGetNickname,
}
