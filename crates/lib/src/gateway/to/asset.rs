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
  #[ts(type = "Uint8Array")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub request_id: Uuid,
}

/// Successful response to `AssetPushBegin`. `resume_from_offset` is the
/// byte offset the next `AssetPushChunk` should start at: 0 for fresh
/// pushes, or the daemon's recovered partial length for a resume.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct AssetPushBeginAck {
  pub resume_from_offset: u32,
}

/// Domain-error response to `AssetPushBegin`: the daemon refuses to
/// start or resume this push (conflicting in-flight id, budget
/// exhausted, oversized, etc.).
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct AssetPushBeginRejected {
  pub reason: String,
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
  #[bridge_response]
  PushBeginAck(AssetPushBeginAck),
  #[bridge_response]
  PushBeginRejected(AssetPushBeginRejected),
}
