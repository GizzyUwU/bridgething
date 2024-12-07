use serde::{Deserialize, Serialize};

use crate::msg::{SendMessage, StockSend};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum StockStorageRecv {
  Get {
    value_type: String,
    key: String,
  },
  Put {
    value_type: String, // literal 'string' it looks like lol
    key: String,
    value: String,
  },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StockStorageSend {
  #[serde(rename = "settings_response")]
  Response { payload: StockStoragePayload },
}

impl From<StockStorageSend> for SendMessage {
  fn from(val: StockStorageSend) -> Self {
    SendMessage::Stock(StockSend::Storage(val))
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockStoragePayload {
  pub key: String,
  pub value: Option<String>,
  pub value_type: String, // literal 'string' it looks like lol
  pub error: Option<bool>,
}

impl From<StockStoragePayload> for SendMessage {
  fn from(payload: StockStoragePayload) -> Self {
    SendMessage::Stock(StockSend::Storage(StockStorageSend::Response { payload }))
  }
}
