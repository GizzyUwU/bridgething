use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{BrightnessState, HardwareState};

/// 0..=100 ambient-brightness indicator derived from the on-board ALS +
/// backlight curve. Low = dark room, high = bright room.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct AmbientLightUpdate {
  pub ambient_level: u8,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Response to `hardware.stateGet`.
pub struct HardwareStateReply {
  pub state: HardwareState,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::BridgeToClientMsgData)]
/// Daemon -> webapp hardware surface: ambient-light and backlight
/// change events, plus the reply to `hardware.stateGet`.
pub enum BridgeToClientHardwareMsg {
  #[bridge_event]
  AmbientLightUpdate(AmbientLightUpdate),
  #[bridge_event]
  BrightnessChanged(BrightnessState),
  #[bridge_response]
  StateReply(HardwareStateReply),
}
