use std::path::PathBuf;

use libbridgething::{server::ServerSystemEvent, BridgeThingMeta, ServerEventData};
use serde::{Deserialize, Serialize};

const BRIDGETHING_VERSION: &str = env!("CARGO_PKG_VERSION");

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

impl From<SuperbirdMeta> for ServerEventData {
  fn from(meta: SuperbirdMeta) -> Self {
    ServerEventData::System(ServerSystemEvent::Version {
      serial: meta.serial_number,
      os_version: "bridgething".to_string(),
      app_version: meta.version,
      fw_version: "NixOS".to_string(),
      model_name: meta.model_name,
      fcc_id: meta.fcc_id,
      ic_id: meta.ic_id,
      country: "Thing Labs".to_string(),
      discord: "https://tl.mt/d".to_string(),
      credits: "Joey Eamigh".to_string(),
    })
  }
}

impl From<SuperbirdMeta> for BridgeThingMeta {
  fn from(meta: SuperbirdMeta) -> Self {
    Self {
      bridgething_version: format!("v{}", BRIDGETHING_VERSION),
      libbridgething_version: Self::libbridgething_version(),
      app_name: "(unknown)".to_string(), // TODO: find a way to handle application name
      app_version: "(unknown)".to_string(), // TODO: find a way to handle application version
      os_name: meta.name,
      os_version: meta.version,
      os_description: meta.description,
      bt_mac: meta.bt_mac,
      serial_number: meta.serial_number,
      fcc_id: meta.fcc_id,
      ic_id: meta.ic_id,
      model_name: meta.model_name,
      discord: "https://tl.mt/d".to_string(),
      credits: "Joey Eamigh".to_string(),
    }
  }
}
