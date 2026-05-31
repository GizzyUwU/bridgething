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

/// Playback context the companion's track plays from (playlist / album /
/// artist / show). `kind` is opaque to the daemon - it forwards the
/// string to webapps that render "playing from <name>".
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct EnrichmentContext {
  pub uri: String,
  pub name: Option<String>,
  pub kind: Option<String>,
}

/// Non-authoritative decoration the iOS companion offers for the track
/// iAP2 says is playing. `anchor_pid` is the iAP2 `persistent_id` the
/// companion echoes from the last `PlaybackHint`, so the daemon can match
/// this offer to the live iAP2 identity by exact equality. `head` is the
/// companion's current Spotify track; `queue` is upcoming. The companion
/// never claims authority - the daemon overlays art / uri / queue onto the
/// iAP2 identity only when the offer provably describes the playing track.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct NowPlayingEnrichment {
  pub anchor_pid: Option<String>,
  pub head: Option<QueueItem>,
  pub queue: Vec<QueueItem>,
  pub context: Option<EnrichmentContext>,
}

/// Gateway -> bridge player events. `Snapshot` is the initial-state event
/// fired at announce when the companion claims player authority;
/// `Delta` is the ongoing partial-update stream (the only delta-shaped
/// event in the wire protocol - every other surface uses snapshots).
/// `QueueChanged` fires when the queue mutates without a track change
/// (companion-side reorder, prefetch). `EnrichmentOffer` is the iOS
/// non-authoritative decoration path (see `NowPlayingEnrichment`).
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
  #[bridge_event]
  EnrichmentOffer(NowPlayingEnrichment),
}
