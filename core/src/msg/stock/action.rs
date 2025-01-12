use serde::{Deserialize, Serialize};

use crate::msg::SystemRecv;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum StockActionRecv {
  VersionRequest,
  RcsRequest,
}

impl From<StockActionRecv> for SystemRecv {
  fn from(data: StockActionRecv) -> Self {
    match data {
      StockActionRecv::VersionRequest => SystemRecv::VersionRequest,
      StockActionRecv::RcsRequest => SystemRecv::__LegacyStockRemoteConfigurationRequest,
    }
  }
}
