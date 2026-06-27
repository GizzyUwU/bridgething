use bridgething_macros::{BridgeEnum, WireRequest};
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

use super::transfer::{TransferBody, TransferRef};
use crate::{LogLevel, LogSource, OtaKind, RangePart};

/// Companion's reply to a `BridgeToGatewaySystemMsg::Keepalive`; echoes `seq`.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct KeepaliveAck {
  pub seq: u32,
}

/// Companion-initiated OTA: opens or resumes a streaming push of an
/// update artifact identified by its sha256. The daemon responds with
/// `OtaBeginAck { resume_from_offset }` (the byte offset the first
/// `TransferFragment` should start at, 0 for fresh pushes) or
/// `OtaBeginRejected { reason }`.
///
/// `kind` selects the backend. See `OtaKind`.
///
/// `update_id` is the sha256 of the artifact, hex-encoded. Content-
/// addressed so resume across daemon restarts and retries-after-failure
/// both work without companion-side state to track. `transfer.id` is
/// minted per attempt and only correlates the fragment stream; the
/// daemon binds it to the `update_id`-keyed partial, so a reconnect
/// with a fresh transfer id still resumes the same bytes.
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
  pub transfer: TransferRef,
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

/// Commit every staged bandaid piece (daemon / hub / stock) as one
/// transaction, then restart bridgething.service once. Bandaid pushes
/// (`OtaKind::Daemon`, `OtaKind::BuiltinWebapp`) stage at stream
/// completion (phase reaches `Writing`/100, the daemon does NOT restart); the
/// companion sends `OtaActivate` after the final piece to swap them all
/// live with a single restart. Image OTAs never use this -- they reboot
/// at write completion.
///
/// `expected` is the set of `update_id`s the companion staged this
/// batch. The daemon errors the activate if its staged set does not
/// match exactly, which guards a desync where a daemon crash dropped the
/// in-memory staged set between staging and activation.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct OtaActivate {
  pub expected: Vec<String>,
}

/// Successful response to `OtaAssetRange`. The companion has the asset
/// (or refetched it from `update_url_base`) and serves the resolved
/// ranges in `body`: small results inline, larger ones as a fragment
/// stream whose offsets are stream-relative (0..sum of part lengths,
/// parts concatenated in declaration order - the daemon's HTTP-Range
/// writer maps them to absolute positions via `parts`). `total_size` is
/// the asset's full byte length (for `Content-Range` totals).
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct OtaAssetRangeReply {
  pub total_size: u32,
  pub parts: Vec<RangePart>,
  pub body: TransferBody,
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

/// Pull a one-shot batch of recent daemon log entries over the gateway.
/// Mirrors the client `LogsTail`; both feed the same `LogTap` ring.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = GatewayToBridge,
  surface = System,
  request_variant = LogsTail,
  response = crate::gateway::LogsTailReply,
  response_variant = LogsTailReply,
)]
pub struct LogsTail {
  pub source: LogSource,
  pub levels: Vec<LogLevel>,
  pub filter: Option<String>,
  pub max_lines: u32,
}

/// Open a streaming daemon-log subscription over the gateway. The daemon
/// returns an opaque token; the companion releases it via `LogsUnsubscribe`.
/// Scoped to the gateway peer - auto-released when the peer disconnects.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = GatewayToBridge,
  surface = System,
  request_variant = LogsSubscribe,
  response = crate::gateway::LogsSubscribeReply,
  response_variant = LogsSubscribeReply,
)]
pub struct LogsSubscribe {
  pub source: LogSource,
  pub levels: Vec<LogLevel>,
  pub filter: Option<String>,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct LogsUnsubscribe {
  pub token: String,
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
  #[bridge_command]
  OtaAbandon(OtaAbandon),
  #[bridge_command]
  OtaActivate(OtaActivate),
  #[bridge_command]
  CancelUpdate,
  #[bridge_response]
  OtaAssetRangeReply(OtaAssetRangeReply),
  #[bridge_response]
  OtaAssetRangeRejected(OtaAssetRangeRejected),
  #[bridge_request]
  DeviceGetNickname,
  #[bridge_request]
  DeviceSetNickname(DeviceSetNickname),
  #[bridge_request]
  LogsTail(LogsTail),
  #[bridge_request]
  LogsSubscribe(LogsSubscribe),
  #[bridge_command]
  LogsUnsubscribe(LogsUnsubscribe),
  #[bridge_response]
  KeepaliveAck(KeepaliveAck),
}
