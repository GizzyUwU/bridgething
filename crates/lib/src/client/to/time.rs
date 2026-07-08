use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::TimeInfo;

#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Wraps `TimeInfo` for the wire; used as both the `time.get` reply and
/// the `Changed` event payload.
pub struct TimeSnapshot {
  pub time: TimeInfo,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::BridgeToClientMsgData)]
/// Daemon -> webapp wall-clock surface: an initial snapshot at
/// announce, `Changed` events on tz/locale/clock updates, and the
/// reply to `time.get`.
pub enum BridgeToClientTimeMsg {
  #[bridge_event]
  Changed(TimeSnapshot),
  #[bridge_response]
  Snapshot(TimeSnapshot),
}
