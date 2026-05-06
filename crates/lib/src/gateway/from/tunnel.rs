use bridgething_macros::BridgeEnum;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

use crate::{TunnelClosed, TunnelData, TunnelError};

#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct TunnelOpenReply {}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct TunnelErrorReply {
  pub error: TunnelError,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::GatewayToBridgeMsgData)]
pub enum GatewayToBridgeTunnelMsg {
  #[bridge_response]
  OpenReply(TunnelOpenReply),
  #[bridge_response]
  ErrorReply(TunnelErrorReply),
  #[bridge_event]
  Data(TunnelData),
  #[bridge_event]
  Closed(TunnelClosed),
}
