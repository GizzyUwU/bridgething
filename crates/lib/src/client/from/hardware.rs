use bridgething_macros::{BridgeDispatch, BridgeEnum, WireRequest};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::BrightnessMode;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Payload for `hardware.displaySetMode`: switch backlight control
/// between daemon-driven (`Auto`, following the ALS) and webapp-driven
/// (`Manual`, via `displaySetLevel`).
pub struct DisplaySetMode {
  pub mode: BrightnessMode,
}

/// Set the manual backlight level. Ignored unless `mode == Manual`;
/// callers should pair with `setMode({ Manual })` when forcing a level.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct DisplaySetLevel {
  /// Backlight level in `[0.0, 1.0]`.
  pub level: f32,
}

/// Webapp asks for the current `HardwareState` snapshot (backlight mode
/// / level, ambient light reading).
#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = ClientToBridge,
  surface = Hardware,
  request_variant = StateGet,
  response = crate::client::HardwareStateReply,
  response_variant = StateReply,
)]
pub struct HardwareStateGet;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, TS, BridgeEnum, BridgeDispatch)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::ClientToBridgeMsgData)]
/// Webapp -> daemon hardware surface: display backlight control. Wheel
/// / button / touch input never crosses this wire; the kiosk delivers
/// those directly to the active webapp as chromium keypresses.
pub enum ClientToBridgeHardwareMsg {
  #[bridge_command]
  DisplaySetMode(DisplaySetMode),
  #[bridge_command]
  DisplaySetLevel(DisplaySetLevel),
  #[bridge_request]
  StateGet,
}
