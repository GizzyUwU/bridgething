use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{BridgeThingMeta, Diagnostics, LogEntry, OtaError, OtaFinished, OtaProgress};

/// Current device nickname. `nickname: None` when the user hasn't set
/// one. Reply to `DeviceGetNickname`; daemon also broadcasts this as a
/// `DeviceNicknameChanged` event when the value mutates so webapps stay
/// in sync without polling.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct DeviceNicknameReply {
  pub nickname: Option<String>,
}

/// Reply to `DiagnosticsGet`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct DiagnosticsReply {
  pub diagnostics: Diagnostics,
}

/// Reply to `LogsTail`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct LogsTailReply {
  /// Matching entries in chronological order (oldest first).
  pub entries: Vec<LogEntry>,
}

/// Reply to `LogsSubscribe`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct LogsSubscribeReply {
  /// Opaque handle; pass to `LogsUnsubscribe` to release the subscription.
  pub token: String,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::BridgeToClientMsgData)]
/// Daemon -> webapp system events and replies. `Version` replies to
/// `VersionRequest` with the daemon's `BridgeThingMeta`. `DiagnosticsReply`,
/// `LogsTailReply`, `LogsSubscribeReply`, and `DeviceNickname` are replies
/// to their matching requests. `LogEntry` streams matching lines to a live
/// `LogsSubscribe` subscription. `OtaProgress` / `OtaError` report OTA
/// orchestrator state. `DeviceNicknameChanged` broadcasts whenever the
/// nickname mutates, including from another surface.
pub enum BridgeToClientSystemMsg {
  #[bridge_response]
  Version(Box<BridgeThingMeta>),
  #[bridge_response]
  DiagnosticsReply(DiagnosticsReply),
  #[bridge_response]
  LogsTailReply(LogsTailReply),
  #[bridge_response]
  LogsSubscribeReply(LogsSubscribeReply),
  #[bridge_event]
  LogEntry(LogEntry),
  #[bridge_event]
  OtaProgress(OtaProgress),
  #[bridge_event]
  OtaError(OtaError),
  #[bridge_event]
  OtaFinished(OtaFinished),
  #[bridge_response]
  DeviceNickname(DeviceNicknameReply),
  #[bridge_event]
  DeviceNicknameChanged(DeviceNicknameReply),
}
