use bridgething_macros::BridgeEnum;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

use crate::{NowPlayingUpdate, PlayerState, QueueItem};

/// Snapshot of the player queue as the companion sees it. Sent on
/// queue mutations the gateway can detect (user reorder, queue clear,
/// gapless prefetch landing). The daemon overwrites its cached queue
/// from this and re-broadcasts to webapps.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct QueueSnapshot {
  pub items: Vec<QueueItem>,
}

/// Gateway -> bridge player events. `Snapshot` is the initial-state event
/// fired at announce when the companion claims player authority;
/// `Delta` is the ongoing partial-update stream (the only delta-shaped
/// event in the wire protocol - every other surface uses snapshots).
/// `QueueChanged` fires when the queue mutates without a track change
/// (companion-side reorder, prefetch).
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::GatewayToBridgeMsgData)]
pub enum GatewayToBridgePlayerMsg {
  #[bridge_event]
  Snapshot(PlayerState),
  #[bridge_event]
  Delta(NowPlayingUpdate),
  #[bridge_event]
  QueueChanged(QueueSnapshot),
}
