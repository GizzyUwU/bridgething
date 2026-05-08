use bridgething_macros::WireEvent;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

pub const LIBBRIDGETHING_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Bridge-side identity announce. Daemon sends one of these to every
/// gateway on connect (companion needs to know what daemon it's talking
/// to so it can opt out of unsupported surfaces). The companion's mirror
/// is `GatewayCapabilities::Announce` over in `shared::capabilities`.
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireEvent)]
#[wire(BridgeToGateway)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct BridgeThingMeta {
  pub bridgething_version: String,
  pub libbridgething_version: String,
  pub app_name: String,
  /// Daemon semver (no leading `v`), e.g. `0.8.4`. Compared directly to
  /// the manifest's daemon-component version for OTA hot-swap decisions.
  pub app_version: String,
  pub os_name: String,
  pub os_version: String,
  pub os_description: String,
  pub bt_mac: String,
  pub serial_number: String,
  pub fcc_id: String,
  pub ic_id: String,
  pub model_name: String,
  /// OTA channel the running image was cut on, e.g. `stable` or `dev`.
  /// The companion's poll loop only auto-pushes when its configured
  /// channel matches; a mismatch surfaces a "channel switch needs full
  /// flash" event rather than swapping channels in-band.
  pub channel: String,
  /// Image variant the running image was cut as, e.g. `prod` or `dev`.
  /// Maps to the yocto image recipe name `bridgething-<variant>-image`,
  /// which is what the companion uses to construct the OTA artifact URL
  /// `images/<channel>/<image_version>/bridgething-<variant>-image.{swu,zck}`.
  pub image_variant: String,
  /// Canonical image version (CalVer, e.g. `2026.05.0`). What the
  /// companion compares to the manifest's image-component version.
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

/// What the streamed bytes are going to be applied as. Image kind
/// streams a `.swu` and goes through libswupdate + slot flip + reboot;
/// daemon kind streams a fresh aarch64 daemon binary and goes through
/// the on-disk rotate (`.incoming` -> `.current`, prior `.current` ->
/// `.previous`) followed by a `systemctl restart bridgething.service`.
/// Companions key reboot expectations off this: image means the device
/// power-cycles, daemon means the daemon process restarts and the
/// gateway link drops and reconnects.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum OtaKind {
  Image,
  Daemon,
}

/// Stage of the OTA orchestrator. The phase set is shared between
/// kinds, with daemon-kind emitting a subset.
///
/// Image: `Streaming` (chunk push) -> `Verifying` (sha+size on disk)
/// -> `Writing` (libswupdate streams to slot) -> `Confirming` (flip
/// try-counter) -> `Reboot` (systemd Reboot).
///
/// Daemon: `Streaming` -> `Verifying` -> `Writing` (atomic rename of
/// `.incoming` over `.current`, with prior `.current` rotated to
/// `.previous`) -> `Reboot` (systemctl restart of bridgething.service).
/// `Confirming` is not emitted for daemon-kind: there is no slot
/// try-counter to flip; the rename is the commit point.
#[typeshare]
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

/// Per-phase progress tick. `percent` is 0-100 within the current
/// phase, not the overall flow. `eta_ms` is best-effort remaining time
/// for the phase when the orchestrator can compute it.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
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
#[ts(export, export_to = "shared.ts")]
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
#[ts(export, export_to = "shared.ts")]
pub struct OtaError {
  pub code: OtaErrorCode,
  pub msg: String,
}

/// Half-open byte range the daemon's range proxy asks the companion
/// to serve. Mirrors HTTP `Range: bytes=start-end` semantics: `start`
/// inclusive, `length` bytes. The proxy caps the multi-range count at
/// the daemon edge (loopback) - companions see whatever swupdate's
/// delta downloader emits. Offsets are u32 because OTA artifacts are
/// bounded at 4 GiB end-to-end (matches `OtaBegin.expected_size`).
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct RangeSpec {
  pub start: u32,
  pub length: u32,
}

/// Resolved range the companion is about to stream. `start` and `length`
/// echo the corresponding `RangeSpec`; the bytes follow as
/// `OtaAssetRangeChunk` events on the Bulk lane.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct RangePart {
  pub start: u32,
  pub length: u32,
}
