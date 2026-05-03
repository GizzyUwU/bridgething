use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{CallEndReason, PhoneCall, PhoneError, PhoneState};

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PhoneStateReply {
  pub state: PhoneState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PhoneCallEnded {
  pub call_id: String,
  pub reason: CallEndReason,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PhoneErrorReply {
  pub error: PhoneError,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::BridgeToClientMsgData)]
pub enum BridgeToClientPhoneMsg {
  #[bridge_event]
  CallStarted(PhoneCall),
  #[bridge_event]
  CallUpdated(PhoneCall),
  #[bridge_event]
  CallEnded(PhoneCallEnded),
  #[bridge_response]
  StateReply(PhoneStateReply),
  #[bridge_response]
  ErrorReply(PhoneErrorReply),
}
