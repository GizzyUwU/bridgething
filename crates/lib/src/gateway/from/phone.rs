use bridgething_macros::BridgeEnum;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{CallEndReason, CommunicationsState, PhoneCall, PhoneError, PhoneState};

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct PhoneErrorReply {
  pub error: PhoneError,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct PhoneStateReply {
  pub state: PhoneState,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct CommunicationsSnapshot {
  pub state: CommunicationsState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct PhoneCallEnded {
  pub call_id: String,
  pub reason: CallEndReason,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::GatewayToBridgeMsgData)]
pub enum GatewayToBridgePhoneMsg {
  #[bridge_event]
  Snapshot(PhoneStateReply),
  #[bridge_event]
  CommunicationsSnapshot(CommunicationsSnapshot),
  #[bridge_event]
  CallStarted(PhoneCall),
  #[bridge_event]
  CallUpdated(PhoneCall),
  #[bridge_event]
  CallEnded(PhoneCallEnded),
  #[bridge_event]
  ErrorEvent(PhoneErrorReply),
  #[bridge_response]
  StateReply(PhoneStateReply),
}
