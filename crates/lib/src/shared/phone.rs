//! Phone surface — telephony state from the connected companion (iAP2
//! call CSMs on iOS, Android via gateway). Replaces the loose
//! `PhoneCallInfo`/`PhoneCallAccept`/`PhoneCallEnd` shapes that lived on
//! `system.rs`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum PhoneCallStatus {
  Disconnected,
  Sending,
  Ringing,
  Connecting,
  Active,
  Held,
  Disconnecting,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum PhoneCallDirection {
  Incoming,
  Outgoing,
}

/// One telephony call. `call_id` is companion-stable for the call's
/// lifetime; webapps pass it back to `answer`/`decline`/`end`/`hold`.
/// `remote_id` is the raw E.164 (or platform raw); `display_name` is the
/// gateway's resolved contact name when available.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct PhoneCall {
  pub call_id: String,
  pub remote_id: String,
  pub display_name: String,
  pub status: PhoneCallStatus,
  pub direction: PhoneCallDirection,
  pub started_at_unix_s: Option<u32>,
}

/// Snapshot of every active call known to the gateway. Multi-call is
/// possible (call-waiting, conference) — webapps rendering only one
/// active call typically pick the first non-Held entry.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct PhoneState {
  pub active_calls: Vec<PhoneCall>,
}

/// Why a call ended, surfaced on `onPhoneCallEnded`. `Failed` carries a
/// platform-defined reason (network, busy, etc.).
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum CallEndReason {
  Local,
  Remote,
  Missed,
  Failed { reason: String },
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum PhoneError {
  /// The supplied `call_id` is not in the daemon's active set.
  CallNotFound { call_id: String },
  /// The companion or platform refused the action (e.g. answer while no
  /// ringing call exists, end on a remote-controlled conference leg).
  ActionRejected { reason: String },
}
