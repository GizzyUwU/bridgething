use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{CallEndReason, CommunicationsState, PhoneCall, PhoneError, PhoneState};

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Response to `stateGet`.
pub struct PhoneStateReply {
  pub state: PhoneState,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Broadcast whenever `CommunicationsState` changes: signal, registration,
/// or which call-control verbs are currently legal.
pub struct PhoneCommunicationsReply {
  pub state: CommunicationsState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PhoneCallEnded {
  /// Echoes `PhoneCall.call_id` for the call that just ended.
  pub call_id: String,
  pub reason: CallEndReason,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Response when a phone command could not be carried out.
pub struct PhoneErrorReply {
  pub error: PhoneError,
}

/// Daemon -> webapp telephony surface. `CallStarted`/`CallUpdated`/
/// `CallEnded`/`CommunicationsChanged` are unsolicited broadcasts driven by
/// the connected companion's telephony state; `StateReply`/`ErrorReply`
/// answer webapp-issued requests and commands.
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
  #[bridge_event]
  CommunicationsChanged(PhoneCommunicationsReply),
  #[bridge_event]
  ErrorEvent(PhoneErrorReply),
  #[bridge_response]
  StateReply(PhoneStateReply),
  #[bridge_response]
  ErrorReply(PhoneErrorReply),
}
