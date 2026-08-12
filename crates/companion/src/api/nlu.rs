use libbridgething::{NluAmount, NluDirection, NluPlaybackSpeed, NluRepeatMode, NluView};

use crate::voice::{fast_path, intent_catalog, rejection};

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "companion.ts")]
pub enum NluFastPathRepeatMode {
  Off,
  All,
  One,
}

impl From<NluRepeatMode> for NluFastPathRepeatMode {
  fn from(mode: NluRepeatMode) -> Self {
    match mode {
      NluRepeatMode::Off => Self::Off,
      NluRepeatMode::All => Self::All,
      NluRepeatMode::One => Self::One,
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "companion.ts")]
pub enum NluFastPathSpeed {
  One,
  OnePointTwo,
  OnePointFive,
  Two,
}

impl From<NluPlaybackSpeed> for NluFastPathSpeed {
  fn from(speed: NluPlaybackSpeed) -> Self {
    match speed {
      NluPlaybackSpeed::One => Self::One,
      NluPlaybackSpeed::OnePointTwo => Self::OnePointTwo,
      NluPlaybackSpeed::OnePointFive => Self::OnePointFive,
      NluPlaybackSpeed::Two => Self::Two,
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "companion.ts")]
pub enum NluFastPathDirection {
  Up,
  Down,
}

impl From<NluDirection> for NluFastPathDirection {
  fn from(direction: NluDirection) -> Self {
    match direction {
      NluDirection::Up => Self::Up,
      NluDirection::Down => Self::Down,
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "companion.ts")]
pub enum NluFastPathAmount {
  Small,
  Medium,
  Large,
}

impl From<NluAmount> for NluFastPathAmount {
  fn from(amount: NluAmount) -> Self {
    match amount {
      NluAmount::Small => Self::Small,
      NluAmount::Medium => Self::Medium,
      NluAmount::Large => Self::Large,
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "companion.ts")]
pub enum NluFastPathView {
  NowPlaying,
  Artist,
  Album,
  Playlist,
  Playlists,
  Library,
  Songs,
  Presets,
  Queue,
  SavedEpisodes,
  NewEpisodes,
}

impl From<NluView> for NluFastPathView {
  fn from(view: NluView) -> Self {
    match view {
      NluView::NowPlaying => Self::NowPlaying,
      NluView::Artist => Self::Artist,
      NluView::Album => Self::Album,
      NluView::Playlist => Self::Playlist,
      NluView::Playlists => Self::Playlists,
      NluView::Library => Self::Library,
      NluView::Songs => Self::Songs,
      NluView::Presets => Self::Presets,
      NluView::Queue => Self::Queue,
      NluView::SavedEpisodes => Self::SavedEpisodes,
      NluView::NewEpisodes => Self::NewEpisodes,
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "companion.ts")]
pub struct NluFastPathSlots {
  pub preset: Option<String>,
  pub level: Option<u32>,
  pub speed: Option<NluFastPathSpeed>,
  pub seconds: Option<i32>,
  pub repeat_mode: Option<NluFastPathRepeatMode>,
  pub enabled: Option<bool>,
  pub view: Option<NluFastPathView>,
  pub mute: Option<bool>,
  pub direction: Option<NluFastPathDirection>,
  pub amount: Option<NluFastPathAmount>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "companion.ts")]
pub struct NluFastPathHit {
  pub intent: String,
  pub slots: NluFastPathSlots,
}

#[uniffi::export]
pub fn nlu_fast_path_match(transcript: String) -> Option<NluFastPathHit> {
  fast_path::match_transcript(&transcript).map(|hit| NluFastPathHit {
    intent: hit.intent.to_owned(),
    slots: NluFastPathSlots {
      preset: hit.slots.preset,
      level: hit.slots.level,
      speed: hit.slots.speed.map(Into::into),
      seconds: hit.slots.seconds,
      repeat_mode: hit.slots.repeat_mode.map(Into::into),
      enabled: hit.slots.enabled,
      view: hit.slots.view.map(Into::into),
      mute: hit.slots.mute,
      direction: hit.slots.direction.map(Into::into),
      amount: hit.slots.amount.map(Into::into),
    },
  })
}

#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "companion.ts")]
pub struct NluRejectionPolicy {
  #[uniffi(default = 0.5)]
  pub in_domain_threshold: f64,
  #[uniffi(default = 0.15)]
  pub clarify_margin: f64,
  #[uniffi(default = 2)]
  pub max_alternates: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export, export_to = "companion.ts")]
pub enum NluRejectionOutcome {
  Accept { intent: String },
  NoIntent,
  Clarify { alternates: Vec<String> },
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum NluRejectionError {
  #[error("{0}")]
  HeadMismatch(String),
}

#[uniffi::export]
pub fn nlu_rejection_evaluate(
  intent_logits: Vec<f64>,
  in_domain_logit: f64,
  policy: NluRejectionPolicy,
) -> Result<NluRejectionOutcome, NluRejectionError> {
  let output = rejection::InferenceOutput {
    intent_logits,
    in_domain_logit,
    slots: libbridgething::NluSlots::default(),
  };
  let policy = rejection::RejectionPolicy {
    in_domain_threshold: policy.in_domain_threshold,
    clarify_margin: policy.clarify_margin,
    max_alternates: policy.max_alternates as usize,
  };
  match rejection::evaluate(&output, policy) {
    Ok(rejection::RejectionOutcome::Accept { intent }) => Ok(NluRejectionOutcome::Accept {
      intent: intent.to_owned(),
    }),
    Ok(rejection::RejectionOutcome::NoIntent) => Ok(NluRejectionOutcome::NoIntent),
    Ok(rejection::RejectionOutcome::Clarify { alternates }) => Ok(NluRejectionOutcome::Clarify {
      alternates: alternates.into_iter().map(str::to_owned).collect(),
    }),
    Err(error) => Err(NluRejectionError::HeadMismatch(error.to_string())),
  }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "companion.ts")]
pub struct NluIntentCatalog {
  pub surface_names: Vec<String>,
  pub no_intent: String,
  pub clarify: String,
}

#[uniffi::export]
pub fn nlu_intent_catalog() -> NluIntentCatalog {
  NluIntentCatalog {
    surface_names: intent_catalog::SURFACE_NAMES
      .iter()
      .map(|name| (*name).to_owned())
      .collect(),
    no_intent: intent_catalog::NO_INTENT.to_owned(),
    clarify: intent_catalog::CLARIFY.to_owned(),
  }
}
