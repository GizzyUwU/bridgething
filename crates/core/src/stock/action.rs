use libbridgething::client::ClientToBridgeSystemMsg;
use serde::{Deserialize, Serialize};

use crate::handler::client::RecvMsgData;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum StockActionRecv {
  VersionRequest,
  RcsRequest,
}

impl From<StockActionRecv> for RecvMsgData {
  fn from(data: StockActionRecv) -> Self {
    match data {
      StockActionRecv::VersionRequest => RecvMsgData::System(ClientToBridgeSystemMsg::VersionRequest),
      StockActionRecv::RcsRequest => RecvMsgData::Unsupported(crate::handler::client::PossibleRecvMsg::Stock(
        super::StockRecvMsg::Action(data),
      )),
    }
  }
}
