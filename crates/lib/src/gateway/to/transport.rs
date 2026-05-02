use bridgething_macros::BridgeEnum;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

use crate::shared::RepeatMode;

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct ShuffleSet {
  pub on: bool,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct RepeatSet {
  pub mode: RepeatMode,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct SeekToSet {
  pub position_ms: u32,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct SkipToIndexSet {
  pub index: u32,
}

/// Bridge-side outbound transport command targeting the connected companion.
/// The companion-side SDK dispatches each verb to its native player
/// integration (Spotify SDK, Apple Music, MediaSession, etc).
///
/// Routing decision lives in `core::transport::TransportController`; this
/// surface only carries the typed verb. The controller emits Transport when
/// the companion has claimed `NowPlayingPlayback` authority; iAP2 HID is
/// the alternate path when authority is held by iAP2.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::BridgeToGatewayMsgData)]
pub enum BridgeToGatewayTransportMsg {
  #[bridge_command]
  Play,
  #[bridge_command]
  Pause,
  #[bridge_command]
  PlayPause,
  #[bridge_command]
  Next,
  #[bridge_command]
  Prev,
  #[bridge_command]
  VolumeUp,
  #[bridge_command]
  VolumeDown,
  #[bridge_command]
  MuteToggle,
  #[bridge_command]
  Shuffle(ShuffleSet),
  #[bridge_command]
  Repeat(RepeatSet),
  #[bridge_command]
  SeekTo(SeekToSet),
  #[bridge_command]
  SkipToIndex(SkipToIndexSet),
}
