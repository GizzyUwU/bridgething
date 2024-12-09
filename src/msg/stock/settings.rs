use serde::{Deserialize, Serialize};

use crate::msg::{PossibleSendMsg, StockSendMsg, StorageRecv, StorageSend};

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

impl From<StockStorageRecv> for StorageRecv {
  fn from(data: StockStorageRecv) -> Self {
    match data {
      StockStorageRecv::Get { key, .. } => StorageRecv::Get { key },
      StockStorageRecv::Put { key, value, .. } => StorageRecv::Put { key, value },
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StockStorageSend {
  #[serde(rename = "settings_response")]
  Response { payload: StockStoragePayload },
}

impl From<StorageSend> for StockStorageSend {
  fn from(data: StorageSend) -> Self {
    match data {
      StorageSend::Response { key, value } => StockStorageSend::Response {
        payload: StockStoragePayload {
          key,
          value,
          value_type: "string".to_owned(),
          error: None,
        },
      },
    }
  }
}

impl From<StockStorageSend> for PossibleSendMsg {
  fn from(val: StockStorageSend) -> Self {
    PossibleSendMsg::Stock(StockSendMsg::Storage(val))
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockStoragePayload {
  pub key: String,
  pub value: Option<String>,
  pub value_type: String, // literal 'string' it looks like lol
  pub error: Option<bool>,
}

impl From<StockStoragePayload> for PossibleSendMsg {
  fn from(payload: StockStoragePayload) -> Self {
    PossibleSendMsg::Stock(StockSendMsg::Storage(StockStorageSend::Response { payload }))
  }
}
