use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

use crate::gateway::BridgeToGatewayMsgData;

pub const LIBBRIDGETHING_VERSION: &str = env!("CARGO_PKG_VERSION");

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct BridgeThingMeta {
  pub bridgething_version: String,
  pub libbridgething_version: String,
  pub app_name: String,
  pub app_version: String,
  pub os_name: String,
  pub os_version: String,
  pub os_description: String,
  pub bt_mac: String,
  pub serial_number: String,
  pub fcc_id: String,
  pub ic_id: String,
  pub model_name: String,
  pub image_build_id: String,
  pub image_build_date: String,
  pub image_distro: String,
  pub image_distro_version: String,
  pub image_machine: String,
  pub discord: String,
  pub credits: String,
}

impl BridgeThingMeta {
  pub fn libbridgething_version() -> String {
    format!("v{}", LIBBRIDGETHING_VERSION)
  }
}

impl From<BridgeThingMeta> for BridgeToGatewayMsgData {
  fn from(data: BridgeThingMeta) -> Self {
    BridgeToGatewayMsgData::Version(data)
  }
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct GatewayMeta {
  pub adapter_version: String,
  pub lib_version: String,
  pub libbridgething_version: String,
  pub app_name: String,
  pub app_version: String,
  pub os_name: String,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "shared.ts")]
pub enum PhoneCallStatus {
  Disconnected,
  Sending,
  Ringing,
  Connecting,
  Active,
  Held,
  Disconnecting,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "shared.ts")]
pub enum PhoneCallDirection {
  Incoming,
  Outgoing,
}
