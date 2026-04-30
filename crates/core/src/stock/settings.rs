use libbridgething::{client::ClientKVStoreCommand, server::{ServerStorageEvent, StorageResponse}};
use serde::{Deserialize, Serialize};


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

impl From<StockStorageRecv> for ClientKVStoreCommand {
  fn from(data: StockStorageRecv) -> Self {
    match data {
      StockStorageRecv::Get { key, .. } => ClientKVStoreCommand::Get { key },
      StockStorageRecv::Put { key, value, .. } => ClientKVStoreCommand::Put { key, value },
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StockStorageSend {
  #[serde(rename = "settings_response")]
  Response { payload: StockStoragePayload },
}

impl From<ServerStorageEvent> for StockStorageSend {
  fn from(data: ServerStorageEvent) -> Self {
    match data {
      ServerStorageEvent::Response(StorageResponse { key, value }) => StockStorageSend::Response {
        payload: StockStoragePayload {
          key,
          value,
          value_type: "string".to_string(),
          error: None,
        },
      },
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockStoragePayload {
  pub key: String,
  pub value: Option<String>,
  pub value_type: String, // literal 'string' it looks like lol
  pub error: Option<bool>,
}
