//! Hardware surface - the bits a webapp can drive on the device itself.
//! Wheel/button/touch input bypass the wire (chromium keypresses to the
//! active webapp); the only on-wire hardware is display backlight and
//! the ALS reading.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

#[typeshare]
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum BrightnessMode {
  /// Daemon drives backlight from the on-board ALS.
  #[default]
  Auto,
  /// Webapp drives backlight directly via `setLevel`.
  Manual,
}

/// Backlight state. `level` is the user-set value (only respected in
/// `Manual`); `effective_level` is what's actually on the panel - equal
/// to `level` in `Manual`, ALS-derived in `Auto`.
#[typeshare]
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct BrightnessState {
  pub mode: BrightnessMode,
  pub level: f32,
  pub effective_level: f32,
}

/// Snapshot of the device's hardware-controlled surfaces. Sent on
/// `hardware.state.get` and re-broadcast on any change. See
/// `AmbientLightUpdate` for the ambient_level semantics.
#[typeshare]
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct HardwareState {
  pub brightness: BrightnessState,
  pub ambient_level: u8,
}

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum HardwareError {
  /// The supplied level is outside `[0.0, 1.0]`.
  LevelOutOfRange,
  /// `setLevel` was called while in `Auto` mode - ignored, switch to
  /// `Manual` first.
  ModeMismatch,
}
