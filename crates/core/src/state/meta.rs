use std::path::PathBuf;

use libbridgething::{BridgeThingMeta, client::BridgeToClientMsgData, gateway::BridgeToGatewayMsgData};
use serde::{Deserialize, Serialize};

const BRIDGETHING_VERSION: &str = env!("CARGO_PKG_VERSION");
const BRIDGETHING_APP_NAME: &str = env!("CARGO_PKG_NAME");

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SuperbirdMeta {
  pub name: String,
  pub version: String,
  pub description: String,
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
}

impl SuperbirdMeta {
  pub async fn read_or_default() -> Self {
    #[cfg(debug_assertions)]
    let meta_path = PathBuf::from("./resources/superbird.json");
    #[cfg(not(debug_assertions))]
    let meta_path = PathBuf::from("/etc/superbird");

    if meta_path.exists() {
      let Ok(data) = tokio::fs::read(&meta_path).await else {
        tracing::warn!(
          "could not find superbird metadata! bridgething is only officially supported on nixos-superbird."
        );
        return Self::default();
      };

      if let Ok(meta) = serde_json::from_slice(&data) {
        meta
      } else {
        tracing::warn!(
          "could not find superbird metadata! bridgething is only officially supported on nixos-superbird."
        );
        Self::default()
      }
    } else {
      tracing::warn!("could not find superbird metadata! bridgething is only officially supported on nixos-superbird.");
      Self::default()
    }
  }
}

impl From<SuperbirdMeta> for BridgeToClientMsgData {
  fn from(meta: SuperbirdMeta) -> Self {
    BridgeThingMeta::from(meta).into()
  }
}

impl From<SuperbirdMeta> for BridgeToGatewayMsgData {
  fn from(meta: SuperbirdMeta) -> Self {
    BridgeThingMeta::from(meta).into()
  }
}

impl From<SuperbirdMeta> for BridgeThingMeta {
  fn from(meta: SuperbirdMeta) -> Self {
    Self {
      bridgething_version: format!("v{}", BRIDGETHING_VERSION),
      libbridgething_version: Self::libbridgething_version(),
      app_name: BRIDGETHING_APP_NAME.to_string(),
      app_version: format!("v{}", BRIDGETHING_VERSION),
      os_name: meta.name,
      os_version: meta.version,
      os_description: meta.description,
      bt_mac: meta.bt_mac,
      serial_number: meta.serial_number,
      fcc_id: meta.fcc_id,
      ic_id: meta.ic_id,
      model_name: meta.model_name,
      image_build_id: meta.image_build_id,
      image_build_date: meta.image_build_date,
      image_distro: meta.image_distro,
      image_distro_version: meta.image_distro_version,
      image_machine: meta.image_machine,
      discord: "https://tl.mt/d".to_string(),
      credits: "Joey Eamigh".to_string(),
    }
  }
}
