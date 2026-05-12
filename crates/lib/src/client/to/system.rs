use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{BridgeThingMeta, Diagnostics, LogEntry, OtaError, OtaProgress};

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct DiagnosticsReply {
  pub diagnostics: Diagnostics,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct LogsTailReply {
  pub entries: Vec<LogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct LogsSubscribeReply {
  pub token: String,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::BridgeToClientMsgData)]
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
  #[bridge_response]
  DeviceNickname(DeviceNicknameReply),
  /// event broadcast when the nickname changes (set via gateway)
  #[bridge_event]
  DeviceNicknameChanged(DeviceNicknameReply),
}
