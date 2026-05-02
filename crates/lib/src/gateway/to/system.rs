use bridgething_macros::BridgeEnum;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

/// Stage of the OTA orchestrator. `Downloading` covers the daemon-side
/// wait for the .swu blob to land in the asset cache (the companion is
/// the actual downloader); `Verifying` runs the sha256 + size check;
/// `Writing` streams to libswupdate; `Confirming` flips slot try-counter
/// state; `Reboot` is the terminal stage emitted just before the daemon
/// triggers the reboot.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum OtaPhase {
  Downloading,
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
/// orchestrator is back to idle and a fresh `ApplyUpdate` may be sent.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum OtaErrorCode {
  /// Companion sent `ApplyUpdate` for an `asset_id` not in the daemon cache.
  AssetNotFound,
  /// Cached blob's sha256 does not match the manifest's expected hash.
  HashMismatch,
  /// Cached blob's byte length does not match the manifest's expected size.
  SizeMismatch,
  /// `CancelUpdate` arrived during a cancelable phase.
  Cancelled,
  /// libswupdate rejected the .swu (parse / handler / I/O failure).
  WriteFailed,
  /// Slot-flip / try-counter reset failed after a successful write.
  ConfirmFailed,
  /// Anything else (asset cache I/O, internal channel close, etc.).
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
}
