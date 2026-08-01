use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct NluConfidence {
  pub intent: String,
  pub slots: Option<String>,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct NluAlternate {
  pub intent: String,
  pub slots: Option<NluSlots>,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct NluSlots {
  pub artist: Option<String>,
  pub track: Option<String>,
  pub album: Option<String>,
  pub playlist: Option<String>,
  pub podcast: Option<String>,
  pub episode: Option<String>,
  pub mood: Option<String>,
  pub genre: Option<String>,
  pub era: Option<String>,
  pub popularity_filter: Option<String>,
  pub entity_type: Option<String>,
  pub query: Option<String>,
  pub webapp_name: Option<String>,
  pub preset: Option<String>,
  pub enabled: Option<bool>,
  pub repeat_mode: Option<NluRepeatMode>,
  pub seconds: Option<i32>,
  pub speed: Option<NluPlaybackSpeed>,
  pub direction: Option<NluDirection>,
  pub amount: Option<String>,
  pub level: Option<u32>,
  pub brightness_mode: Option<NluBrightnessMode>,
  pub view: Option<NluView>,
  pub phone_action: Option<NluPhoneAction>,
  pub system_action: Option<NluSystemAction>,
  pub uri: Option<String>,
}

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum NluRepeatMode {
  Off,
  All,
  One,
}

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum NluPlaybackSpeed {
  #[serde(rename = "1")]
  One,
  #[serde(rename = "1.2")]
  OnePointTwo,
  #[serde(rename = "1.5")]
  OnePointFive,
  #[serde(rename = "2")]
  Two,
}

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum NluDirection {
  Up,
  Down,
}

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum NluBrightnessMode {
  Auto,
  Manual,
}

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum NluView {
  Library,
  Presets,
  Songs,
  SavedEpisodes,
  NewEpisodes,
  Queue,
  ThisArtist,
}

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum NluPhoneAction {
  Answer,
  Decline,
  End,
  Hold,
  Unhold,
  Swap,
  Merge,
  Mute,
  Unmute,
}

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum NluSystemAction {
  Reboot,
  PowerOff,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct NluResolvedIntent {
  pub intent: String,
  #[serde(default)]
  pub slots: NluSlots,
  pub transcript: String,
  pub confidence: Option<NluConfidence>,
  pub alternates: Option<Vec<NluAlternate>>,
}

#[typeshare]
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum VoiceCaptureReason {
  #[default]
  PushToTalk,
  Assistant,
  WakeWord,
}

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum NluStage {
  FastPath,
  Model,
  RejectedNoIntent,
  RejectedClarify,
  NoModel,
}

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum VoiceDispatchErrorCode {
  WebappNotFound,
  NotDispatchable,
  Unsupported,
  PlaybackFailed,
  BadSlots,
  Internal,
}

/// Where the daemon actually routed a successful dispatch
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum VoiceDispatchTarget {
  /// Transport / playback effect
  Playback,
  /// A daemon-local device setting (brightness, discoverable, power).
  Device,
  /// Phone call control over the companion's phone surface.
  Phone,
  /// Display-shaped intent handed to the active webapp to render.
  Display,
  /// Switched the active webapp via OPEN_WEBAPP.
  WebappSwitch,
}
