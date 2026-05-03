use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{NowPlayingUpdate, PlayerError, PlayerState, QueueItem};

#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PlayerStateReply {
  pub state: PlayerState,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PlayerQueueReply {
  pub items: Vec<QueueItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PlayerErrorReply {
  pub error: PlayerError,
}

/// Daemon → webapp player surface. `Snapshot` lands on connect with the
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
