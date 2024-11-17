use serde::{Deserialize, Serialize};

use super::{SendMessage, StockDeviceType, StockSend};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StockConnectionSend {
  #[serde(rename = "remote_control_connection_status")]
  RemoteStatus {
    payload: bool,
    mac: String,
    phone_type: StockDeviceType,
  },
  #[serde(rename = "transport_connection_status")]
  TransportStatus { payload: bool },
}

impl From<StockConnectionSend> for SendMessage {
  fn from(val: StockConnectionSend) -> Self {
    SendMessage::Stock(StockSend::Connection(val))
  }
}
