use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

/// One timed line. `start_ms` is relative to track start, so a webapp highlights against the
/// same clock it draws the progress bar from.
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct LyricLine {
  pub start_ms: u32,
  pub text: String,
}

/// Lyrics for one track. `synced` and `plain` are independent: a source may carry either, both,
/// or neither. `source` names the provider so a webapp can attribute it.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct Lyrics {
  pub synced: Option<Vec<LyricLine>>,
  pub plain: Option<String>,
  pub source: String,
}
