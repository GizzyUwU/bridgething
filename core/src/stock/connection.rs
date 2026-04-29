use serde::{Deserialize, Serialize};

use super::StockDeviceType;
use super::StockSendMsg;
use crate::handler::client::PossibleSendMsg;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StockConnectionSend {
  #[serde(rename = "remote_control_connection_status")]
  RemoteStatus {
    payload: bool,
    mac: String,
    phone_type: StockDeviceType,
  },
  #[serde(rename = "remote_control_app_change")]
  RemoteApp { app_id: String, is_spotify: bool },
  #[serde(rename = "transport_connection_status")]
  TransportStatus { payload: bool },
}

impl From<StockConnectionSend> for StockSendMsg {
  fn from(val: StockConnectionSend) -> Self {
    Self::Connection(val)
  }
}

impl From<StockConnectionSend> for PossibleSendMsg {
  fn from(val: StockConnectionSend) -> Self {
    Self::Stock(StockSendMsg::Connection(val))
  }
}
