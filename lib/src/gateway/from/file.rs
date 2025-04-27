use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::gateway::BridgeFile;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(
  tag = "type",
  content = "data",
  rename_all = "camelCase",
  rename_all_fields = "camelCase"
)]
#[ts(export, export_to = "gateway.ts")]
pub enum GatewayToBridgeFileMsg {
  List, // request
  Delete {
    files: Vec<String>,
  }, // command
  Add {
    #[debug(skip)]
    files: Vec<BridgeFile>,
  }, // command
}
