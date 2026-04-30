use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

use crate::gateway::{BridgeToGatewayMsgData, FileResponseData, GatewayToBridgeFileMsg, GatewayToBridgeMsgData};
use crate::impl_bridge_request;

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct FileRequestData {
  pub file: String,
}

/// Bridge-side request for a runtime file the gateway has access to.
/// Triggered by HTTP misses inside the `/_gateway/` namespace. The gateway
/// replies with `GatewayToBridgeFileMsg::FileResponse`.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum BridgeToGatewayFileMsg {
  FileRequest(FileRequestData),
}

impl_bridge_request! {
  request: FileRequestData,
  response: FileResponseData,
  encode_request:
    r => BridgeToGatewayMsgData::File(BridgeToGatewayFileMsg::FileRequest(r)),
  extract_response:
    GatewayToBridgeMsgData::File(GatewayToBridgeFileMsg::FileResponse(v)) => v,
  encode_response:
    v => GatewayToBridgeMsgData::File(GatewayToBridgeFileMsg::FileResponse(v)),
}
