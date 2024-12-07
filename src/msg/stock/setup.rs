use serde::{Deserialize, Serialize};

use crate::msg::{SendMessage, StockSend};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StockSetupSend {
  #[serde(rename = "setup_status")]
  Status { payload: String },
}

impl From<StockSetupSend> for SendMessage {
  fn from(val: StockSetupSend) -> Self {
    SendMessage::Stock(StockSend::Setup(val))
  }
}
