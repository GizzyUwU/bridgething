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
  Files {
    files: Vec<String>,
  }, // response
  /// fileRequest occurs when a file is requested over http that is not known to the bridge
  FileRequest {
    file: String,
  }, // request
}

impl From<BridgeToGatewayFileMsg> for BridgeToGatewayMsgData {
  fn from(val: BridgeToGatewayFileMsg) -> Self {
    BridgeToGatewayMsgData::File(val)
  }
}
