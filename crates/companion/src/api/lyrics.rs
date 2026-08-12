use crate::lyrics::lrc;

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "companion.ts")]
pub struct LrcLine {
  pub start_ms: u32,
  pub text: String,
}

#[uniffi::export]
pub fn parse_lrc(text: String) -> Vec<LrcLine> {
  lrc::parse(&text)
    .into_iter()
    .map(|line| LrcLine {
      start_ms: line.start_ms,
      text: line.text,
    })
    .collect()
}
