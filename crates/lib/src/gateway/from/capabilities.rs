use bridgething_macros::BridgeEnum;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

use crate::GatewayCapabilities;

/// Companion-driven capabilities surface. The companion sends `Announce`
/// immediately on session-up (before any other surface activity) and
/// re-sends on any change. The daemon's PeerTracker flips
/// `companion_active` on receipt and seeds initial snapshots for every
/// surface where the companion is claiming authority.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::GatewayToBridgeMsgData)]
pub enum GatewayToBridgeCapabilitiesMsg {
  #[bridge_event]
  Announce(GatewayCapabilities),
}
