use bridgething_macros::{BridgeDispatch, BridgeEnum, WireRequest};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Payload for `voice.muteMic`.
pub struct MicMute {
  /// When true and a capture session is already in progress, let it
  /// keep running instead of cutting it short.
  pub preserve: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Payload for `voice.unmuteMic`.
pub struct MicUnmute {
  /// Accepted for symmetry with `MicMute`; the daemon ignores it on
  /// unmute.
  pub preserve: bool,
}

/// Webapp asks for the current `VoiceState` (muted / capturing).
#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = ClientToBridge,
  surface = Voice,
  request_variant = StateGet,
  response = crate::client::VoiceStateReply,
  response_variant = StateReply,
)]
pub struct VoiceStateGet;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum, BridgeDispatch)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::ClientToBridgeMsgData)]
/// Webapp -> daemon voice/NLU surface: mic mute control and manual
/// capture triggering. May be unavailable on builds without the mic
/// hardware or the NLU pipeline enabled.
pub enum ClientToBridgeVoiceMsg {
  #[bridge_command]
  Cancel,
  #[bridge_command]
  PushToTalk,
  #[bridge_command]
  MuteMic(MicMute),
  #[bridge_command]
  UnmuteMic(MicUnmute),
  #[bridge_request]
  StateGet,
}
