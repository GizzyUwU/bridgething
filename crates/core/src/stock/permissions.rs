use serde::{Deserialize, Serialize};

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StockPermissionsSend {
  DevicePermissions {
    can_use_superbird: bool,
    can_play_on_demand: Option<bool>,
  },
}
