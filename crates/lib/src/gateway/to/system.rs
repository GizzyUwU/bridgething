use bridgething_macros::{BridgeEnum, WireRequest};
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

use crate::{OtaError, OtaProgress, RangeSpec};

/// Successful response to `OtaBegin`. `resume_from_offset` is the byte
/// offset the next `OtaChunk` should start at: 0 for fresh pushes, or
/// the daemon's recovered partial length for a resume.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct OtaBeginAck {
  pub resume_from_offset: u32,
}

/// Domain-error response to `OtaBegin`: the daemon refuses to start
/// or resume this push (already-running OTA, conflicting in-flight
/// update_id with mismatched size/sha, budget exhausted).
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct OtaBeginRejected {
  pub reason: String,
}

/// Daemon asks the pinned companion to serve byte ranges from an asset
/// it should have cached (and can refetch from `OtaBegin.update_url_base`
/// on cache miss). Triggered by an inbound HTTP-Range request from
/// libswupdate's delta downloader hitting the daemon's loopback proxy.
/// Range count is bounded daemon-side; companions just serve whatever
/// arrives.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = BridgeToGateway,
  surface = System,
  request_variant = OtaAssetRange,
  response = crate::gateway::OtaAssetRangeReply,
  response_variant = OtaAssetRangeReply,
  error = crate::gateway::OtaAssetRangeRejected,
  error_variant = OtaAssetRangeRejected,
)]
pub struct OtaAssetRange {
  pub update_id: String,
  pub asset: String,
  pub ranges: Vec<RangeSpec>,
}

/// Daemon-side cancel for an in-flight range request: libcurl gave up
/// (timeout, OTA failed, daemon is shutting down). Companion stops
/// sending `OtaAssetRangeChunk` events for `request_id` and frees any
/// resources it held open.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct OtaAssetRangeAbandon {
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
pub enum BridgeToGatewaySystemMsg {
  #[bridge_event]
  OtaProgress(OtaProgress),
  #[bridge_event]
  OtaError(OtaError),
  #[bridge_response]
  OtaBeginAck(OtaBeginAck),
  #[bridge_response]
  OtaBeginRejected(OtaBeginRejected),
  #[bridge_request]
  OtaAssetRange(OtaAssetRange),
  #[bridge_command]
  OtaAssetRangeAbandon(OtaAssetRangeAbandon),
}
