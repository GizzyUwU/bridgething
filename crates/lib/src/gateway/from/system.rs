use bridgething_macros::{BridgeEnum, WireRequest};
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

/// Companion-initiated OTA: opens or resumes a streaming push of a
/// `.swu` artifact identified by its sha256. The daemon responds with
/// `OtaBeginAck { resume_from_offset }` (the byte offset the next
/// `OtaChunk` should start at, 0 for fresh pushes) or
/// `OtaBeginRejected { reason }` (already-running OTA, conflicting
/// in-flight update_id with mismatched size/sha, or budget exhausted).
///
/// `update_id` is the sha256 of the .swu, hex-encoded. Content-addressed
/// so resume across daemon restarts and retries-after-failure both work
/// without companion-side state to track.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = GatewayToBridge,
  surface = System,
  request_variant = OtaBegin,
  response = crate::gateway::OtaBeginAck,
  response_variant = OtaBeginAck,
  error = crate::gateway::OtaBeginRejected,
  error_variant = OtaBeginRejected,
)]
pub struct OtaBegin {
  pub update_id: String,
  pub manifest_url: Option<String>,
  pub expected_sha256: String,
  pub expected_size: u32,
}

/// Streaming chunk of a .swu push opened by `OtaBegin`. `offset` must
/// equal the daemon's current `received` for the transfer (chunks are
/// strictly in-order; the companion learns the resume offset from
/// `OtaBeginAck`). `last:true` triggers post-stream verify (size +
/// sha256) followed by `Verifying`/`Writing`/`Confirming`/`Reboot`
/// phase progress events.
#[typeshare]
#[serde_with::serde_as]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct OtaChunk {
  pub update_id: String,
  pub offset: u32,
  #[debug(skip)]
  #[serde_as(as = "serde_with::Bytes")]
  #[ts(type = "Uint8Array")]
  pub bytes: Vec<u8>,
  pub last: bool,
}

/// Drop the daemon-side partial for `update_id`. After `CancelUpdate`
/// keeps the partial for resume; `OtaAbandon` is the explicit clean-up
/// when the companion no longer wants to retry this artifact.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct OtaAbandon {
  pub update_id: String,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::GatewayToBridgeMsgData)]
pub enum GatewayToBridgeSystemMsg {
  #[bridge_request]
  OtaBegin(OtaBegin),
  #[bridge_event]
  OtaChunk(OtaChunk),
  #[bridge_command]
  OtaAbandon(OtaAbandon),
  #[bridge_command]
  CancelUpdate,
}
