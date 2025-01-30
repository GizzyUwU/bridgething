use libbridgething::client::ClientSystemCommand;
use serde::{Deserialize, Serialize};

use crate::msg::RecvMsgData;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum StockActionRecv {
  VersionRequest,
  RcsRequest,
}

impl From<StockActionRecv> for RecvMsgData {
  fn from(data: StockActionRecv) -> Self {
    match data {
      StockActionRecv::VersionRequest => RecvMsgData::System(ClientSystemCommand::VersionRequest),
      StockActionRecv::RcsRequest => {
        RecvMsgData::Unsupported(crate::msg::PossibleRecvMsg::Stock(super::StockRecvMsg::Action(data)))
      }
    }
  }
}
