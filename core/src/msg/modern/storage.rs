use serde::{Deserialize, Serialize};

use crate::msg::SendMsgData;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", content = "args", rename_all = "camelCase")]
pub enum StorageRecv {
  Get { key: String },
  Put { key: String, value: String },
  Delete { key: String },
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", content = "data", rename_all = "camelCase")]
pub enum StorageSend {
  Response { key: String, value: Option<String> },
}

impl From<StorageSend> for SendMsgData {
  fn from(val: StorageSend) -> Self {
    SendMsgData::Storage(val)
  }
}
