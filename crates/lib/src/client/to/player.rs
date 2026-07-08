use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{CurrentlyActiveApplication, NowPlayingUpdate, PlayerError, PlayerState, QueueItem};

#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Response to `stateGet`, also carried by the `Snapshot` event.
pub struct PlayerStateReply {
  pub state: PlayerState,
  /// The app currently driving playback, when known (iOS surfaces this over iAP2).
  pub active_app: Option<CurrentlyActiveApplication>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Response to `queueGet`, also carried by the `QueueChanged` event.
pub struct PlayerQueueReply {
  /// The now-playing track, when one is loaded.
  pub current: Option<QueueItem>,
  /// Upcoming tracks in queue order.
  pub items: Vec<QueueItem>,
  /// Recently-played history.
  pub previous: Vec<QueueItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PlayerErrorReply {
  pub error: PlayerError,
}

/// Daemon -> webapp player surface. `Snapshot` lands on connect with the
/// current player state; `Delta` is the `NowPlayingUpdate` stream the
/// SDK auto-merges; `QueueChanged` fires when the queue mutates without
/// a track change. `StateReply`/`QueueReply` are the typed responses
/// to the matching webapp queries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::BridgeToClientMsgData)]
pub enum BridgeToClientPlayerMsg {
  #[bridge_event]
  Snapshot(PlayerStateReply),
  #[bridge_event]
  Delta(NowPlayingUpdate),
  #[bridge_event]
  QueueChanged(PlayerQueueReply),
  #[bridge_response]
  StateReply(PlayerStateReply),
  #[bridge_response]
  QueueReply(PlayerQueueReply),
  #[bridge_response]
  ErrorReply(PlayerErrorReply),
}
