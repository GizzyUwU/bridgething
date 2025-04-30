use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::gateway::BridgeToGatewayMsgData;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(
  tag = "event",
  content = "data",
  rename_all = "camelCase",
  rename_all_fields = "camelCase"
)]
#[ts(export, export_to = "gateway.ts")]
pub enum BridgeToGatewayFileMsg {
  Files { files: Vec<String> }, // response
}

impl From<BridgeToGatewayFileMsg> for BridgeToGatewayMsgData {
  fn from(val: BridgeToGatewayFileMsg) -> Self {
    BridgeToGatewayMsgData::File(val)
  }
}
