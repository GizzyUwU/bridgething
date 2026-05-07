use bridgething_macros::BridgeEnum;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

use crate::{PlayContext, QueuePosition, RepeatMode};

/// Play a URI on the gateway. `context` lets the gateway honor playlist
/// / album semantics for skip-next when both sides understand the
/// scheme. The daemon parses the scheme and only forwards if a
/// connected gateway claims it; otherwise returns `PlayerError::SchemeUnclaimed`.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct PlayUri {
  pub uri: String,
  pub context: Option<PlayContext>,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct QueueUri {
  pub uri: String,
  pub position: QueuePosition,
}

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct SeekTo {
  pub position_ms: u32,
}

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct SkipToIndex {
  pub index: u32,
}

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct SetShuffle {
  pub on: bool,
}

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct SetRepeat {
  pub mode: RepeatMode,
}

/// Set absolute playback rate. `1.0` is normal speed; gateways with
/// limited speed support clamp to their nearest supported value.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct SetSpeed {
  pub speed: f32,
}

/// `duration_ms = None` turns crossfade off; `Some(0)` is also off but
/// distinguishes intent.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct SetCrossfade {
  pub duration_ms: Option<u32>,
}

/// Bridge → gateway player verbs. The companion-side SDK dispatches each
/// to its native player integration (Spotify SDK, Apple Music SDK,
/// MediaSession). Routing for `Play(uri)` is gated on
/// `Capabilities.uri_schemes` - daemon never forwards a URI no
/// connected gateway claims.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::BridgeToGatewayMsgData)]
pub enum BridgeToGatewayPlayerMsg {
  #[bridge_command]
  Play(PlayUri),
  #[bridge_command]
  Queue(QueueUri),
  #[bridge_command]
  Pause,
  #[bridge_command]
  Resume,
  #[bridge_command]
  SkipNext,
  #[bridge_command]
  SkipPrev,
  #[bridge_command]
  SkipToIndex(SkipToIndex),
  #[bridge_command]
  SeekTo(SeekTo),
  #[bridge_command]
  SetShuffle(SetShuffle),
  #[bridge_command]
  SetRepeat(SetRepeat),
  #[bridge_command]
  SetSpeed(SetSpeed),
  #[bridge_command]
  SetCrossfade(SetCrossfade),
}
