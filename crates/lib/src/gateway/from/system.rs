use bridgething_macros::{BridgeEnum, WireRequest};
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

use crate::{OtaKind, RangePart};

/// Companion-initiated OTA: opens or resumes a streaming push of an
/// update artifact identified by its sha256. The daemon responds with
/// `OtaBeginAck { resume_from_offset }` (the byte offset the next
/// `OtaChunk` should start at, 0 for fresh pushes) or
/// `OtaBeginRejected { reason }`.
///
/// `kind` selects the backend. See `OtaKind`.
///
/// `update_id` is the sha256 of the artifact, hex-encoded. Content-
/// addressed so resume across daemon restarts and retries-after-failure
/// both work without companion-side state to track.
///
/// `update_url_base` is image-kind only: the server prefix the companion
/// may refetch the .zck delta from on cache miss while serving range
/// requests during the Writing phase. Ignored for non-image kinds.
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
  pub kind: OtaKind,
  pub update_id: String,
  pub update_url_base: Option<String>,
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

/// Successful response to `OtaAssetRange`. The companion has the asset
/// (or refetched it from `update_url_base`) and is about to stream the
/// requested ranges as `OtaAssetRangeChunk` events on the Bulk lane.
/// `parts` echoes the resolved ranges in the order they will be sent;
/// `total_size` is the asset's full byte length (for `Content-Range`
/// totals).
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct OtaAssetRangeReply {
  pub total_size: u32,
  pub parts: Vec<RangePart>,
}

/// Domain-error response to `OtaAssetRange`: the companion can't serve
/// the requested ranges (asset unknown for this `update_id`, refetch
/// from `update_url_base` failed, sha mismatch on refetched asset, etc).
/// The daemon surfaces this to libswupdate as a `502 Bad Gateway` and
/// the running OTA fails.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct OtaAssetRangeRejected {
  pub reason: String,
}

/// Streaming bytes for one part of an `OtaAssetRange` reply. Sent on
/// the Bulk lane in order: parts in declaration order, chunks in
/// ascending `offset`. `offset` is absolute within the asset, not
/// within the part - matches what the daemon's HTTP-Range writer needs
/// to feed libcurl. `last:true` only on the final chunk of the final
/// part for this `request_id`.
#[typeshare]
#[serde_with::serde_as]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct OtaAssetRangeChunk {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub request_id: Uuid,
  pub part_index: u32,
  pub offset: u32,
  #[debug(skip)]
  #[serde_as(as = "serde_with::Bytes")]
  #[ts(type = "Uint8Array")]
  pub bytes: Vec<u8>,
  pub last: bool,
}

/// Gateway-side read of the device nickname. Returns the current value
/// (or None when unset). Daemon also broadcasts `DeviceNicknameChanged`
/// to gateway peers on mutation so the companion stays in sync without
/// polling.
#[typeshare]
#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = GatewayToBridge,
  surface = System,
  request_variant = DeviceGetNickname,
  response = crate::gateway::DeviceNicknameReply,
  response_variant = DeviceNickname,
)]
pub struct DeviceGetNickname;

/// Set the device nickname. Empty string clears (treated as None). The
/// daemon broadcasts `DeviceNicknameChanged` to gateway + client peers
/// after writing the KV slot. Length-capped at 64 chars.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = GatewayToBridge,
  surface = System,
  request_variant = DeviceSetNickname,
  response = crate::gateway::DeviceNicknameReply,
  response_variant = DeviceNickname,
  error = crate::gateway::DeviceNicknameRejected,
  error_variant = DeviceNicknameRejected,
)]
pub struct DeviceSetNickname {
  pub nickname: String,
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
  #[bridge_response]
  OtaAssetRangeReply(OtaAssetRangeReply),
  #[bridge_response]
  OtaAssetRangeRejected(OtaAssetRangeRejected),
  #[bridge_event]
  OtaAssetRangeChunk(OtaAssetRangeChunk),
  #[bridge_request]
  DeviceGetNickname,
  #[bridge_request]
  DeviceSetNickname(DeviceSetNickname),
}
