use bridgething_macros::BridgeEnum;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct SetVolume {
  pub level: f32,
}

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct SetMute {
  pub muted: bool,
}

/// Fire-and-forget TTS request. `id` is webapp-assigned (no request
/// round-trip) and used for cancellation + matching back-to-back
/// `TtsStarted`/`TtsEnded` events. `voice` selects from
/// `AudioCapabilities.voices`; `None` uses the gateway's default.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct Tts {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub id: Uuid,
  pub text: String,
  pub voice: Option<String>,
}

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct TtsCancel {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub id: Uuid,
}

/// Play a named earcon from `AudioCapabilities.earcons`. Unknown names
/// surface as `AudioError::EarconNotFound`.
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct Earcon {
  pub name: String,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::BridgeToGatewayMsgData)]
pub enum BridgeToGatewayAudioMsg {
  #[bridge_command]
  VolumeUp,
  #[bridge_command]
  VolumeDown,
  #[bridge_command]
  SetVolume(SetVolume),
  #[bridge_command]
  MuteToggle,
  #[bridge_command]
  SetMute(SetMute),
  #[bridge_command]
  Tts(Tts),
  #[bridge_command]
  TtsCancel(TtsCancel),
  #[bridge_command]
  TtsCancelAll,
  #[bridge_command]
  Earcon(Earcon),
}
