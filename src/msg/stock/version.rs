use serde::{Deserialize, Serialize};

use crate::msg::{PossibleSendMsg, StockSendMsg};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StockVersionSend {
  #[serde(rename = "version_status")]
  Status {
    serial: String,
    os_version: String,
    app_version: String,
    touch_fw_version: String,
    model_name: String,
    fcc_id: String,
    ic_id: String,
  },
}

impl From<StockVersionSend> for StockSendMsg {
  fn from(val: StockVersionSend) -> Self {
    Self::Version(val)
  }
}

impl From<StockVersionSend> for PossibleSendMsg {
  fn from(val: StockVersionSend) -> Self {
    Self::Stock(StockSendMsg::Version(val))
  }
}
