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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum BridgeToGatewayTransportMsg {
  Play,
  Pause,
  PlayPause,
  Next,
  Prev,
  VolumeUp,
  VolumeDown,
  MuteToggle,
  Shuffle(ShuffleSet),
  Repeat(RepeatSet),
  SeekTo(SeekToSet),
  SkipToIndex(SkipToIndexSet),
}
