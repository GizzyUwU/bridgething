use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "stock.ts")]
pub struct StockSetPreset {
  pub version: usize, // 1
  pub context_uri: String,
  pub slot_index: usize, // 1-4
  pub source: String,    // 'tactile' | 'voice'
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "stock.ts")]
pub struct StockPreset {
  pub context_uri: String,
  pub image_url: Option<String>,
  pub slot_index: usize, // 1-4
  pub name: Option<String>,
  pub description: Option<String>,
}
