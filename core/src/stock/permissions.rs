use serde::{Deserialize, Serialize};

use super::StockSendMsg;
use crate::handler::client::PossibleSendMsg;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StockPermissionsSend {
  DevicePermissions {
    can_use_superbird: bool,
    can_play_on_demand: Option<bool>,
  },
}

impl From<StockPermissionsSend> for StockSendMsg {
  fn from(val: StockPermissionsSend) -> Self {
    Self::Permissions(val)
  }
}

impl From<StockPermissionsSend> for PossibleSendMsg {
  fn from(val: StockPermissionsSend) -> Self {
    Self::Stock(StockSendMsg::Permissions(val))
  }
}
