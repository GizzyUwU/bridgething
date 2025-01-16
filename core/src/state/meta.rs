use libbridgething::{server::ServerSystemEvent, ServerEventData};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
  pub name: String,
  pub version: String,
  pub bt_mac: String,
  pub serial_number: String,
  pub fcc_id: String,
  pub ic_id: String,
  pub model_name: String,
}

impl From<Meta> for ServerEventData {
  fn from(meta: Meta) -> Self {
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
