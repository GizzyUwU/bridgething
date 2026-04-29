use serde::{Deserialize, Serialize};

use super::StockSendMsg;
use crate::handler::client::PossibleSendMsg;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StockVersionSend {
  #[serde(rename = "version_status")]
  Status {
    serial: String,
    os_version: String,
    country: String,
    app_version: String,
    fw_version: String,
    model_name: String,
    fcc_id: String,
    ic_id: String,
    discord: String,
    credits: String,
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
