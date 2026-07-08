use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Payload for `setVolume`.
pub struct SetVolume {
  /// Absolute output level, `0.0` (silent) to `1.0` (max).
  pub level: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Payload for `setMute`.
pub struct SetMute {
  /// `true` mutes output, `false` unmutes.
  pub muted: bool,
}

/// Fire-and-forget TTS request. `id` is webapp-assigned and used both
/// for cancellation and for matching back-to-back `TtsStarted`/`TtsEnded`
/// events. `voice` selects from `AudioCapabilities.voices`; `None` uses
/// the gateway default.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct Tts {
  #[ts(type = "string")]
  pub id: Uuid,
  pub text: String,
  pub voice: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Payload for `ttsCancel`.
pub struct TtsCancel {
  /// Id of the in-flight `Tts` request to cancel, as passed to `Tts.id`.
  #[ts(type = "string")]
  pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Payload for `earcon`.
pub struct Earcon {
  /// Earcon name; must be one of `AudioCapabilities.earcons`.
  pub name: String,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::ClientToBridgeMsgData)]
/// Webapp -> daemon audio control surface: volume/mute, TTS playback, and
/// earcons. All fire-and-forget; `setVolume`/`setMute` broadcast a
/// `VolumeChanged` event, and TTS lifecycle is reported via
/// `TtsStarted`/`TtsEnded`.
pub enum ClientToBridgeAudioMsg {
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
