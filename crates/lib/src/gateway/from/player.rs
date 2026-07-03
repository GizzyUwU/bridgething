use bridgething_macros::BridgeEnum;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

use crate::{PlayerState, QueueItem};

/// Full queue replacement from the companion. `order` is the upcoming
/// queue as a list of item uris, current excluded; `items` carries full
/// metadata for every uri in `order`, so the daemon rebuilds the queue
/// from the snapshot alone. The companion sends one only when the upcoming
/// list materially changes (context switch, reorder, add-to-queue), never
/// on a plain advance - the daemon derives the post-advance next by
/// locating the now-playing track in the held queue. Both sides drop this
/// state on disconnect; no deltas, revisions, or resync to track.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct QueueSnapshot {
  pub order: Vec<String>,
  pub items: Vec<QueueItem>,
}

/// Gateway -> bridge player events. The companion is authoritative for
/// now-playing: `Snapshot` carries the full player state and is the sole
/// metadata/playback source (driven by the dealer push). `QueueChanged`
/// carries a full queue replacement when the upcoming list materially
/// changes; the daemon derives the post-advance next from the held
/// snapshot, so a plain advance costs no queue traffic.
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
  #[bridge_command]
  RequestSpotifyWake,
}
