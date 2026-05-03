use bridgething_macros::BridgeEnum;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

/// Stage of the OTA orchestrator. `Streaming` covers the chunk-by-chunk
/// push of the `.swu` from companion to daemon-disk; `Verifying` runs
/// the post-stream sha256 + size check; `Writing` streams the on-disk
/// `.swu` to libswupdate; `Confirming` flips slot try-counter state;
/// `Reboot` is the terminal stage emitted just before the daemon
/// triggers the reboot.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum OtaPhase {
  Streaming,
  Verifying,
  Writing,
  Confirming,
  Reboot,
}

/// Per-phase progress tick. `percent` is 0-100 within the current
/// phase, not the overall flow. `eta_ms` is best-effort remaining time
/// for the phase when the orchestrator can compute it.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct OtaProgress {
  pub phase: OtaPhase,
  pub percent: u8,
  pub eta_ms: Option<u32>,
}

/// Terminal error from the OTA orchestrator. After an `OtaError` the
/// orchestrator is back to idle and a fresh `OtaBegin` may be sent.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum OtaErrorCode {
  /// Companion sent chunks for an `update_id` that was never begun
  /// (or was abandoned mid-stream).
  UnknownUpdate,
  /// `OtaChunk.offset` did not match the daemon's `received`.
  OffsetMismatch,
  /// Streamed total's sha256 did not match `OtaBegin.expected_sha256`.
  HashMismatch,
  /// Streamed total's byte length did not match `OtaBegin.expected_size`.
  SizeMismatch,
  /// `CancelUpdate` arrived during a cancelable phase.
  Cancelled,
  /// libswupdate rejected the .swu (parse / handler / I/O failure).
  WriteFailed,
  /// Slot-flip / try-counter reset failed after a successful write.
  ConfirmFailed,
  /// Anything else (transfer-cache I/O, internal channel close, etc.).
  Internal,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct OtaError {
  pub code: OtaErrorCode,
  pub msg: String,
}

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
}
