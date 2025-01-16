use libbridgething::client::ClientSystemCommand;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum StockActionRecv {
  VersionRequest,
  RcsRequest,
}

impl From<StockActionRecv> for ClientSystemCommand {
  fn from(data: StockActionRecv) -> Self {
    match data {
      StockActionRecv::VersionRequest => ClientSystemCommand::VersionRequest,
      StockActionRecv::RcsRequest => ClientSystemCommand::__LegacyStockRemoteConfigurationRequest,
    }
  }
}
