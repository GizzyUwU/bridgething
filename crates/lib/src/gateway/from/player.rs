use bridgething_macros::BridgeEnum;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

use crate::{PlaybackTarget, PlayerState, QueueItem};

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct QueueSnapshot {
  pub order: Vec<String>,
  pub items: Vec<QueueItem>,
}

/// Complete replacement list of the provider's remote endpoints.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct PlaybackTargets {
  pub targets: Vec<PlaybackTarget>,
}

#[typeshare]
#[allow(clippy::large_enum_variant)]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::GatewayToBridgeMsgData)]
pub enum GatewayToBridgePlayerMsg {
  #[bridge_event]
  Snapshot(PlayerState),
  #[bridge_event]
  QueueChanged(QueueSnapshot),
  #[bridge_event]
  TargetsChanged(PlaybackTargets),
  #[bridge_command]
  RequestSpotifyWake,
}
