use bridgething_macros::BridgeEnum;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

/// Fired when the companion has begun speaking the TTS request with this
/// id. May arrive after `TtsEnded` is dropped (e.g. companion preempted
/// before speech started); webapps should treat both as best-effort.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct TtsStarted {
  #[ts(type = "Uint8Array")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub id: Uuid,
}

/// Fired when the TTS request finished. `completed` is true when the
/// full text was spoken; false when preempted, cancelled, or the
/// companion dropped it.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct TtsEnded {
  #[ts(type = "Uint8Array")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub id: Uuid,
  pub completed: bool,
}

/// Volume / mute snapshot. Fired on any change to either; webapps treat
/// `level` as the canonical value.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct VolumeChanged {
  pub level: f32,
  pub muted: bool,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::GatewayToBridgeMsgData)]
pub enum GatewayToBridgeAudioMsg {
  #[bridge_event]
  TtsStarted(TtsStarted),
  #[bridge_event]
  TtsEnded(TtsEnded),
  #[bridge_event]
  VolumeChanged(VolumeChanged),
}
