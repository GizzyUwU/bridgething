use bridgething_macros::{BridgeEnum, WireRequest};
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

use crate::AssetRetention;

/// Maximum payload size for a single-frame `Push`. Pushes carrying
/// more than this must use the chunked `PushBegin` / `PushChunk` flow,
/// which streams to disk one chunk at a time and never accumulates the
/// full payload in daemon memory. Sized to comfortably cover album
/// art (~200-300 KB on iOS) without leaving room for a misbehaving
/// companion to push multi-megabyte blobs through the memory path.
pub const ASSET_PUSH_SINGLE_FRAME_MAX_BYTES: usize = 256 * 1024;

/// Single-frame asset push for small, latency-critical, memory-resident
/// assets (album art). The payload size must be at most
/// `ASSET_PUSH_SINGLE_FRAME_MAX_BYTES` and `retention` must not be
/// `Persistent`. Larger payloads or persistent retention require the
/// chunked `PushBegin`/`PushChunk` flow.
#[typeshare]
#[serde_with::serde_as]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct AssetPush {
  pub id: String,
  #[debug(skip)]
  #[serde_as(as = "serde_with::Bytes")]
  #[ts(type = "Uint8Array")]
  pub bytes: Vec<u8>,
  pub mime: Option<String>,
  pub retention: AssetRetention,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct AssetClear {
  pub id: String,
}

/// Open or resume a chunked asset push. Daemon responds with
/// `AssetPushBeginAck { resume_from_offset }` (the byte offset the next
/// `AssetPushChunk` should start at, 0 for fresh pushes) or
/// `AssetPushBeginRejected { reason }` (conflicting in-flight id with
/// mismatched size/sha, budget exhausted, etc.).
///
/// Required for any push with `retention = Persistent` and for any push
/// larger than `ASSET_PUSH_SINGLE_FRAME_MAX_BYTES`.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = GatewayToBridge,
  surface = Asset,
  request_variant = PushBegin,
  response = crate::gateway::AssetPushBeginAck,
  response_variant = PushBeginAck,
  error = crate::gateway::AssetPushBeginRejected,
  error_variant = PushBeginRejected,
)]
pub struct AssetPushBegin {
  pub id: String,
  pub expected_size: u32,
  pub expected_sha256: Option<String>,
  pub mime: Option<String>,
  pub retention: AssetRetention,
}

/// Streaming chunk of an asset push opened by `AssetPushBegin`.
/// `offset` must equal the daemon's current `received` for this id.
/// `last:true` triggers post-stream verify (size + optional sha256)
/// and commit to the asset cache.
#[typeshare]
#[serde_with::serde_as]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct AssetPushChunk {
  pub id: String,
  pub offset: u32,
  #[debug(skip)]
  #[serde_as(as = "serde_with::Bytes")]
  #[ts(type = "Uint8Array")]
  pub bytes: Vec<u8>,
  pub last: bool,
}

/// Drop the daemon-side partial for `id`. The companion's escape hatch
/// when it wants to clean up a push it can no longer complete.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct AssetPushAbandon {
  pub id: String,
}

/// Typed response payload for an `AssetRequest`. Mirrors `AssetPush`
/// without the retention hint - the daemon picks retention for assets
/// it asked for, since the lifecycle is request-scoped rather than
/// companion-managed.
#[typeshare]
#[serde_with::serde_as]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct AssetGotReply {
  pub id: String,
  #[debug(skip)]
  #[serde_as(as = "serde_with::Bytes")]
  #[ts(type = "Uint8Array")]
  pub bytes: Vec<u8>,
  pub mime: Option<String>,
}

/// Domain error response for an `AssetRequest`: the companion does not
/// have the requested asset.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct AssetNotFoundReply {
  pub id: String,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::GatewayToBridgeMsgData)]
pub enum GatewayToBridgeAssetMsg {
  #[bridge_event]
  Push(AssetPush),
  #[bridge_event]
  Clear(AssetClear),
  #[bridge_request]
  PushBegin(AssetPushBegin),
  #[bridge_event]
  PushChunk(AssetPushChunk),
  #[bridge_command]
  PushAbandon(AssetPushAbandon),
  #[bridge_response]
  Got(AssetGotReply),
  #[bridge_response]
  NotFound(AssetNotFoundReply),
}
