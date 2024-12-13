use serde::{Deserialize, Serialize};

use crate::msg::{SendMsgData, SystemSend};

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

impl From<Meta> for SendMsgData {
  fn from(meta: Meta) -> Self {
    SendMsgData::System(SystemSend::Version {
      serial: meta.serial_number,
      os_version: "bridgething".to_owned(),
      app_version: meta.version,
      fw_version: "NixOS".to_owned(),
      model_name: meta.model_name,
      fcc_id: meta.fcc_id,
      ic_id: meta.ic_id,
      country: "Thing Labs".to_owned(),
      discord: "https://tl.mt/d".to_owned(),
      credits: "Joey Eamigh".to_owned(),
    })
  }
}
