use serde::{Deserialize, Serialize};

use super::{SendMessage, StockSend};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StockPermissionsSend {
  DevicePermissions {
    can_use_superbird: bool,
    can_play_on_demand: bool,
  },
}

impl From<StockPermissionsSend> for SendMessage {
  fn from(val: StockPermissionsSend) -> Self {
    SendMessage::Stock(StockSend::Permissions(val))
  }
}
