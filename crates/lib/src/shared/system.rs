use bridgething_macros::WireEvent;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

pub const LIBBRIDGETHING_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Bridge-side identity announce. Daemon sends one of these to every
/// gateway on connect (companion needs to know what daemon it's talking
/// to so it can opt out of unsupported surfaces). The companion's mirror
/// is `GatewayCapabilities::Announce` over in `shared::capabilities`.
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireEvent)]
#[wire(BridgeToGateway)]
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
