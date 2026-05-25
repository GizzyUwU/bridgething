use bridgething_macros::BridgeEnum;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

use crate::NluResolvedIntent;

/// Why the gateway is opening the mic. The daemon currently treats every
/// reason the same (open and stream); the field is kept so future policy
/// (hotword vs. assistant routing, VAD timeout per reason) has the shape
/// it needs.
#[typeshare]
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum VoiceCaptureReason {
  #[default]
  PushToTalk,
  Assistant,
  WakeWord,
}

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct VoiceMicOpen {
  pub reason: VoiceCaptureReason,
}

/// The companion has resolved a captured utterance into an NluResolvedIntent
/// (fast-path or LLM stage on the phone, plus SpotifyResolver decoration on
/// catalog slots) and is asking the daemon to dispatch. The daemon's
/// dispatcher picks the target: stock playback, active-webapp forward, or
/// OPEN_WEBAPP switch. Outcome is broadcast via `BridgeToGatewayVoiceMsg::
/// Dispatched` / `DispatchFailed`.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct VoiceDispatch {
  pub resolved: Box<NluResolvedIntent>,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::GatewayToBridgeMsgData)]
pub enum GatewayToBridgeVoiceMsg {
  #[bridge_command]
  MicOpen(VoiceMicOpen),
  #[bridge_command]
  MicClose,
  #[bridge_command]
  Dispatch(VoiceDispatch),
}
