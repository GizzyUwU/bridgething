use bridgething_macros::{BridgeDispatch, BridgeEnum, WireRequest};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{PlayContext, QueuePosition, RepeatMode};

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Payload for the `play` command.
pub struct PlayUri {
  /// Resource to play, e.g. `spotify:track:...`; any uri scheme a connected gateway claims.
  pub uri: String,
  /// Optional album/playlist/show to play `uri` within, so next/prev follow that list.
  pub context: Option<PlayContext>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Payload for the `queue` command.
pub struct QueueUri {
  /// Resource to enqueue.
  pub uri: String,
  /// Where in the queue it lands (append / next / explicit index).
  pub position: QueuePosition,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Payload for `seekTo`: jump to an absolute playhead position.
pub struct SeekTo {
  /// Target playhead in milliseconds from track start.
  pub position_ms: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Payload for `skipToIndex`: jump to a specific row in the current queue.
pub struct SkipToIndex {
  /// 0-based index into the queue.
  pub index: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Payload for `skipPrev`: go to the previous track, or restart the current one.
pub struct SkipPrev {
  /// When true, restart the current track if it is progressed past the restart threshold; otherwise always move to the previous track.
  pub allow_seeking: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Payload for `setShuffle`.
pub struct SetShuffle {
  /// Desired shuffle state.
  pub on: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Payload for `setRepeat`.
pub struct SetRepeat {
  /// Desired repeat mode (off / all / one).
  pub mode: RepeatMode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Payload for `setSpeed`. Honored only by gateways that support rate control.
pub struct SetSpeed {
  /// Playback rate; 1.0 is normal speed.
  pub speed: f32,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Payload for `setCrossfade`.
pub struct SetCrossfade {
  /// Crossfade duration in milliseconds; `None` turns crossfade off.
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

/// Webapp asks for the current queue snapshot. Rarely needed - the SDK
/// tracks the queue from `QueueChanged` events.
#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = ClientToBridge,
  surface = Player,
  request_variant = QueueGet,
  response = crate::client::PlayerQueueReply,
  response_variant = QueueReply,
)]
pub struct PlayerQueueGet;

/// Webapp asks for the provider's remote endpoints
#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = ClientToBridge,
  surface = Player,
  request_variant = TargetsGet,
  response = crate::client::PlayerTargetsReply,
  response_variant = TargetsReply,
)]
pub struct PlayerTargetsGet;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Payload for `transferTo`: move playback to a remote endpoint.
pub struct TransferTo {
  /// A `PlaybackTarget.id` from the current target list.
  pub target_id: String,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS, BridgeEnum, BridgeDispatch)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::ClientToBridgeMsgData)]
/// Webapp -> daemon player control surface. Commands are fire-and-forget;
/// the daemon routes each to whichever gateway owns playback. The two
/// requests fetch a one-shot snapshot.
pub enum ClientToBridgePlayerMsg {
  /// Start playback of a uri, optionally within a context.
  #[bridge_command]
  Play(PlayUri),
  /// Add a uri to the queue.
  #[bridge_command]
  Queue(QueueUri),
  /// Pause playback.
  #[bridge_command]
  Pause,
  /// Resume playback.
  #[bridge_command]
  Resume,
  /// Skip to the next track.
  #[bridge_command]
  SkipNext,
  /// Skip to the previous track, or restart the current one.
  #[bridge_command]
  SkipPrev(SkipPrev),
  /// Jump to a specific queue index.
  #[bridge_command]
  SkipToIndex(SkipToIndex),
  /// Seek to an absolute position.
  #[bridge_command]
  SeekTo(SeekTo),
  /// Toggle shuffle.
  #[bridge_command]
  SetShuffle(SetShuffle),
  /// Set the repeat mode.
  #[bridge_command]
  SetRepeat(SetRepeat),
  /// Change playback speed.
  #[bridge_command]
  SetSpeed(SetSpeed),
  /// Set the crossfade duration.
  #[bridge_command]
  SetCrossfade(SetCrossfade),
  /// Move playback to one of the provider's remote endpoints.
  #[bridge_command]
  TransferTo(TransferTo),
  /// Request the current `PlayerState` snapshot.
  #[bridge_request]
  StateGet,
  /// Request the current queue snapshot.
  #[bridge_request]
  QueueGet,
  /// Request the provider's current remote endpoints.
  #[bridge_request]
  TargetsGet,
}
