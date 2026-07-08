use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Fired when a `Tts` request begins audibly playing.
pub struct TtsStarted {
  /// Echoes `Tts.id`.
  #[ts(type = "string")]
  pub id: Uuid,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Fired when a `Tts` request stops playing, whether it finished or was cut short.
pub struct TtsEnded {
  /// Echoes `Tts.id`.
  #[ts(type = "string")]
  pub id: Uuid,
  /// `true` if playback finished naturally; `false` if cancelled or interrupted.
  pub completed: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Current output level and mute state, broadcast after any change
/// regardless of whether this webapp issued the command.
pub struct VolumeChanged {
  /// Absolute output level, `0.0` (silent) to `1.0` (max).
  pub level: f32,
  pub muted: bool,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::BridgeToClientMsgData)]
/// Daemon -> webapp audio events: TTS lifecycle notifications and
/// volume/mute changes.
pub enum BridgeToClientAudioMsg {
  #[bridge_event]
  TtsStarted(TtsStarted),
  #[bridge_event]
  TtsEnded(TtsEnded),
  #[bridge_event]
  VolumeChanged(VolumeChanged),
}
