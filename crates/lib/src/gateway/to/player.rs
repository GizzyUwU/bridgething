use bridgething_macros::{BridgeEnum, WireRequest};
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{PlayContext, QueuePosition, RepeatMode};

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct PlayUri {
  pub uri: String,
  pub context: Option<PlayContext>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct QueueUri {
  pub uri: String,
  pub position: QueuePosition,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct SeekTo {
  pub position_ms: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct SkipToIndex {
  pub index: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct SetShuffle {
  pub on: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct SetRepeat {
  pub mode: RepeatMode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct SetSpeed {
  pub speed: f32,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct SetCrossfade {
  pub duration_ms: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct TransferTo {
  pub target_id: String,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = BridgeToGateway,
  surface = Player,
  request_variant = SnapshotRequest,
  response = crate::gateway::PlayerSnapshotAck,
  response_variant = SnapshotAck,
  error = crate::gateway::PlayerErrorReply,
  error_variant = ErrorReply,
)]
pub struct PlayerSnapshotRequest {}

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
  #[bridge_command]
  TransferTo(TransferTo),
  #[bridge_request]
  SnapshotRequest(PlayerSnapshotRequest),
}
