use bridgething_macros::{BridgeEnum, WireRequest};
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

#[typeshare]
#[serde_with::serde_as]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = BridgeToGateway,
  surface = Asset,
  request_variant = Request,
  response = crate::gateway::AssetGotReply,
  response_variant = Got,
  error = crate::gateway::AssetNotFoundReply,
  error_variant = NotFound,
)]
pub struct AssetRequest {
  pub id: String,
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub request_id: Uuid,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::BridgeToGatewayMsgData)]
pub enum BridgeToGatewayAssetMsg {
  #[bridge_request]
  Request(AssetRequest),
}
