//! Phone surface - telephony state from the connected companion (iAP2
//! call CSMs on iOS, Android via gateway).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum PhoneCallDirection {
  Incoming,
  Outgoing,
}

/// Call bearer / service kind. iAP2's `CallStateUpdateService` enum
/// values, projected to our wire surface. Companion gateways that don't
/// distinguish bearers project all calls to `Telephony`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum PhoneCallService {
  Unknown,
  Telephony,
  FaceTimeAudio,
  FaceTimeVideo,
}

/// One telephony call. `call_id` is companion-stable for the call's
/// lifetime; webapps pass it back to `answer`/`decline`/`end`/`hold`.
/// `remote_id` is the raw E.164 (or platform raw); `display_name` is the
/// gateway's resolved contact name when available.
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
  pub label: Option<String>,
  pub address_book_id: Option<String>,
  pub service: Option<PhoneCallService>,
  pub is_conferenced: Option<bool>,
  pub conference_group: Option<u8>,
}

/// Snapshot of every active call known to the gateway. Multi-call is
/// possible (call-waiting, conference) - webapps rendering only one
/// active call typically pick the first non-Held entry.
#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct PhoneState {
  pub active_calls: Vec<PhoneCall>,
}

/// Why a call ended, surfaced on `onPhoneCallEnded`. `Failed` carries a
/// platform-defined reason (network, busy, etc.).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum CallEndReason {
  Local,
  Remote,
  Missed,
  Declined,
  Failed { reason: String },
}

/// Cellular registration state - populated from iAP2 `CommunicationsUpdate`
/// or the companion's equivalent.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum RegistrationStatus {
  Unknown,
  NotRegistered,
  Searching,
  Denied,
  RegisteredHome,
  RegisteredRoaming,
  EmergencyCallsOnly,
}

/// What call-control verbs are currently legal. Webapps must gate UI on
/// these flags; sending an unavailable verb is a protocol violation, not
/// a no-op. All `None` = no signal received yet, treat as conservatively
/// unavailable.
#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct CommunicationsState {
  pub signal_strength: Option<u8>,
  pub registration_status: Option<RegistrationStatus>,
  pub airplane_mode: Option<bool>,
  pub carrier_name: Option<String>,
  pub cellular_supported: Option<bool>,
  pub telephony_enabled: Option<bool>,
  pub face_time_audio_enabled: Option<bool>,
  pub face_time_video_enabled: Option<bool>,
  pub mute_status: Option<bool>,
  pub current_call_count: Option<u8>,
  pub new_voicemail_count: Option<u8>,
  pub initiate_call_available: Option<bool>,
  pub end_and_accept_available: Option<bool>,
  pub hold_and_accept_available: Option<bool>,
  pub swap_available: Option<bool>,
  pub merge_available: Option<bool>,
  pub hold_available: Option<bool>,
}

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
  /// No iAP2 link or companion attached, so there's nowhere to send the
  /// outbound action.
  NoTarget,
  /// `*Available` flag for this verb was false at action time.
  Unavailable { verb: String },
}

/// DTMF tones the accessory can play during an active call.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum DtmfTone {
  D0,
  D1,
  D2,
  D3,
  D4,
  D5,
  D6,
  D7,
  D8,
  D9,
  Star,
  Hash,
}

/// Direction the accessory wants iOS to take when answering an incoming
/// call while another call is active.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum AcceptCallAction {
  /// Answer the new call (placing any existing active call on hold if
  /// telephony allows it).
  #[default]
  Accept,
  /// End the existing active call and answer the new one.
  EndAndAccept,
}

/// Direction the accessory wants iOS to take when ending a call.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum EndCallAction {
  /// End / decline the call referenced by `CallUUID`.
  #[default]
  End,
  /// End every active call.
  EndAll,
}

/// What kind of outbound call the accessory wants placed.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum InitiateCallType {
  #[default]
  Destination,
  Voicemail,
  Redial,
}
