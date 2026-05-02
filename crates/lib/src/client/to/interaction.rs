use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Daemon → webapp interaction events. None defined yet; the variant
/// exists on the outer wire so the surface is reserved for future
/// daemon-pushed UI hints (haptic prompt, focus change, etc.).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub enum BridgeToClientInteractionMsg {}
