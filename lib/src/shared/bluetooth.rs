use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct Device {
  pub name: String,
  #[serde(rename = "type")]
  pub device_type: DeviceType,
  pub mac: String,
  pub default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum DeviceType {
  Android,
  #[serde(rename = "iOS")]
  Ios,
  Windows,
  MacOS,
  Linux,
  Unknown,
}
