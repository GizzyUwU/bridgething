//! Geo surface - lat/lon position from the connected companion. Bespoke
//! subscribe model (battery-sensitive); see surface enums for verbs.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

/// One position fix from the gateway. `accuracy_m` is the 1-sigma
/// horizontal radius. `speed_mps` and `heading_deg` are populated when
/// the underlying source provides them (CLLocation on iOS does for moving
/// fixes; Android's FusedLocationProvider similar). `ts_ms` is the
/// gateway-provided fix timestamp, not the wire-arrival time.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct Position {
  pub lat: f64,
  pub lon: f64,
  pub alt_m: Option<f32>,
  pub accuracy_m: f32,
  pub speed_mps: Option<f32>,
  pub heading_deg: Option<f32>,
  pub ts_unix_s: u32,
}

/// Subscriber's accuracy preference. `Coarse` opts into the lower-power
/// city-block-grade fix on platforms that distinguish (iOS reduced
/// accuracy, Android `PRIORITY_BALANCED_POWER_ACCURACY`). The daemon
/// aggregates across subscribers and forwards the most-demanding to the
/// companion.
#[typeshare]
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum GeoAccuracy {
  Coarse,
  #[default]
  Fine,
}

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum GeoError {
  /// The user denied location to the companion app, or the OS gate
  /// refuses (e.g. iOS Always/WhenInUse not granted).
  PermissionDenied,
  /// Companion is connected but cannot produce a fix (no GPS, airplane
  /// mode, indoors with no fallback).
  Unavailable,
  /// The supplied subscription token is unknown to the daemon.
  UnknownToken,
}
