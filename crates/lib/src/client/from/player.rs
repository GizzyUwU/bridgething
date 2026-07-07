use bridgething_macros::{BridgeDispatch, BridgeEnum, WireRequest};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{PlayContext, QueuePosition, RepeatMode};

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PlayUri {
  pub uri: String,
  pub context: Option<PlayContext>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct QueueUri {
  pub uri: String,
  pub position: QueuePosition,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct SeekTo {
  pub position_ms: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct SkipToIndex {
  pub index: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct SkipPrev {
  /// when true, restart the current track if it is progressed past the restart threshold; otherwise always move to the previous track.
  pub allow_seeking: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct SetShuffle {
  pub on: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct SetRepeat {
  pub mode: RepeatMode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct SetSpeed {
  pub speed: f32,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct SetCrossfade {
  pub duration_ms: Option<u32>,
}

/// Webapp asks for the current `PlayerState` snapshot. Most webapps
/// don't need this - the SDK auto-merges deltas into a cached state.
#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = ClientToBridge,
  surface = Player,
  request_variant = StateGet,
  response = crate::client::PlayerStateReply,
  response_variant = StateReply,
)]
pub struct PlayerStateGet;

#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = ClientToBridge,
  surface = Player,
  request_variant = QueueGet,
  response = crate::client::PlayerQueueReply,
  response_variant = QueueReply,
)]
pub struct PlayerQueueGet;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS, BridgeEnum, BridgeDispatch)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::ClientToBridgeMsgData)]
pub enum ClientToBridgePlayerMsg {
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
  SkipPrev(SkipPrev),
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
  #[bridge_request]
  StateGet,
  #[bridge_request]
  QueueGet,
}
