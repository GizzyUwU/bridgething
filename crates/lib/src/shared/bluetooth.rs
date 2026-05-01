use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct Device {
  pub name: String,
  #[serde(rename = "type")]
  #[typeshare(serialized_as = "DeviceType")]
  pub device_type: DeviceType,
  pub mac: String,
  pub default: bool,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
#[derive(Default)]
pub enum DeviceType {
  Android,
  #[serde(rename = "iOS")]
  Ios,
  Windows,
  MacOS,
  Linux,
  #[default]
  Unknown,
}
