use bridgething_macros::WireEvent;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const LIBBRIDGETHING_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Bridge-side identity announce
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireEvent)]
#[wire(BridgeToGateway)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct BridgeThingMeta {
  pub bridgething_version: String,
  pub libbridgething_version: String,
  pub app_name: String,
  pub nickname: Option<String>,
  pub app_version: String,
  pub daemon_sha256: Option<String>,
  /// None when no wake word model resolved, or when the one that did carries no version stamp.
  pub wakeword_model_version: Option<String>,
  pub os_name: String,
  pub os_version: String,
  pub os_description: String,
  pub bt_mac: String,
  pub serial_number: String,
  pub fcc_id: String,
  pub ic_id: String,
  pub model_name: String,
  pub channel: String,
  pub image_variant: String,
  pub image_version: String,
  pub image_build_id: String,
  pub image_build_date: String,
  pub image_distro: String,
  pub image_machine: String,
  pub discord: String,
  pub credits: String,
}

impl BridgeThingMeta {
  pub fn libbridgething_version() -> String {
    format!("v{}", LIBBRIDGETHING_VERSION)
  }
}

/// What the streamed bytes are going to be applied as
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum OtaKind {
  Image,
  Daemon,
  BuiltinWebapp,
  InstalledWebapp,
  WakewordModel,
}

/// Stage of the OTA orchestrator. The phase set is shared between
/// kinds, with non-image kinds emitting a subset
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum OtaPhase {
  Streaming,
  Verifying,
  Writing,
  Confirming,
  Reboot,
}

/// Per-phase progress tick
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct OtaProgress {
  pub phase: OtaPhase,
  pub percent: u8,
  pub step: u8,
  pub nsteps: u8,
  pub dwl_percent: u8,
  pub dwl_bytes: u32,
  pub eta_ms: Option<u32>,
}

/// Terminal error from the OTA orchestrator
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum OtaErrorCode {
  /// Companion sent fragments for an `update_id` that was never begun
  UnknownUpdate,
  /// A fragment's offset did not match the daemon's `received`.
  OffsetMismatch,
  /// Streamed total's sha256 did not match `OtaBegin.transfer.sha256`.
  HashMismatch,
  /// Streamed total's byte length did not match `OtaBegin.transfer.total_size`.
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

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct OtaError {
  pub code: OtaErrorCode,
  pub msg: String,
  /// which update failed. a resume re-drives the same artifact, so this is NOT unique per attempt.
  pub update_id: Option<String>,
  /// set only when the bridge is re-delivering a failure whose peer had already gone away
  #[serde(default, skip_serializing_if = "std::ops::Not::not")]
  pub replayed: bool,
}

/// Terminal success from the OTA orchestrator, emitted for every `OtaKind`.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct OtaFinished {
  pub kind: OtaKind,
  pub update_id: String,
}

/// Half-open byte range the daemon's range proxy asks the companion to serve
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct RangeSpec {
  pub start: u32,
  pub length: u32,
}

/// Resolved range the companion is about to serve
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct RangePart {
  pub start: u32,
  pub length: u32,
}
