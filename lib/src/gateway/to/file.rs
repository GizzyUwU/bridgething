use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

use crate::gateway::BridgeToGatewayMsgData;

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct FileRequestData {
  pub file: String,
}

/// Bridge-side request for a runtime file the gateway has access to.
/// Triggered by HTTP misses inside the `/_gateway/` namespace.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum BridgeToGatewayFileMsg {
  FileRequest(FileRequestData),
}

impl From<BridgeToGatewayFileMsg> for BridgeToGatewayMsgData {
  fn from(val: BridgeToGatewayFileMsg) -> Self {
    BridgeToGatewayMsgData::File(val)
  }
}
