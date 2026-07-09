use bridgething_macros::WireEvent;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

pub const LIBBRIDGETHING_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Bridge-side identity announce. Daemon sends one of these to every
/// gateway on connect so the companion knows what daemon it's talking to
/// and can opt out of unsupported surfaces.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireEvent)]
#[wire(BridgeToGateway)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct BridgeThingMeta {
  pub bridgething_version: String,
  pub libbridgething_version: String,
  pub app_name: String,
  /// User-set display name for this device. None when the user hasn't
  /// set one yet; consumers fall back to `model_name` / `serial_number`.
  /// Set via the gateway-side `system.device.setNickname` surface.
  pub nickname: Option<String>,
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

/// What the streamed bytes are going to be applied as.
///
/// `Image` streams a `.swu` through libswupdate + slot flip + reboot.
/// `Daemon` streams a fresh aarch64 daemon binary, atomic-rotates on
/// the bandaid bind-mount, restarts the service. `BuiltinWebapp`
/// streams a zip bundle of hub or stock, validates the manifest id is
/// one of the reserved built-ins, atomic-rotates the bundle dir on the
/// bandaid bind-mount, restarts the service. `InstalledWebapp` streams
/// a zip bundle of a third-party (non-reserved) webapp and installs it
/// into the writable registry; it neither stages on the bandaid nor
/// restarts, and is never part of an `OtaActivate` batch.
///
/// Companions key reboot expectations off this: image means the device
/// power-cycles; daemon and builtin-webapp mean the daemon process
/// restarts and the gateway link drops and reconnects; installed-webapp
/// applies in place with no restart, and the terminal signal is the
/// `WebappInstalled` event (or an `OtaError`).
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum OtaKind {
  Image,
  Daemon,
  BuiltinWebapp,
  InstalledWebapp,
}

/// Stage of the OTA orchestrator. The phase set is shared between
/// kinds, with non-image kinds emitting a subset.
///
/// Image: `Streaming` -> `Verifying` -> `Writing` (libswupdate to slot)
/// -> `Confirming` (try-counter reset) -> `Reboot`.
///
/// Daemon and BuiltinWebapp: `Streaming` -> `Verifying` -> `Writing`,
/// where `Writing`/100 means the piece is validated and staged on the
/// bandaid (not yet live). The atomic rotate and the single `systemctl
/// restart` happen later, on `OtaActivate`, which emits the terminal
/// `Reboot` for the whole batch. `Confirming` is image-only.
///
/// InstalledWebapp: `Streaming` -> `Verifying` -> `Writing`/0 while the
/// bundle installs into the writable registry. There is no `Writing`/100,
/// no `Confirming`, and no `Reboot`; the terminal signal is the
/// `WebappInstalled` event (or an `OtaError`).
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
/// libswupdate step, which resets at each step boundary, so it is not a
/// monotonic overall metric on its own. `eta_ms` is best-effort remaining
/// time for the phase when the orchestrator can compute it.
#[typeshare]
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

/// Terminal error from the OTA orchestrator. After an `OtaError` the
/// orchestrator is back to idle and a fresh `OtaBegin` may be sent.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum OtaErrorCode {
  /// Companion sent fragments for an `update_id` that was never begun
  /// (or was abandoned mid-stream).
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

/// Resolved range the companion is about to serve. `start` and `length`
/// echo the corresponding `RangeSpec`; the bytes follow in the reply's
/// `TransferBody`.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct RangePart {
  pub start: u32,
  pub length: u32,
}
