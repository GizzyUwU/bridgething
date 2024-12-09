use serde::{Deserialize, Serialize};

use crate::msg::{PossibleSendMsg, StockSendMsg};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StockSetupSend {
  #[serde(rename = "setup_status")]
  Status { payload: String },
}

impl From<StockSetupSend> for PossibleSendMsg {
  fn from(val: StockSetupSend) -> Self {
    PossibleSendMsg::Stock(StockSendMsg::Setup(val))
  }
}
