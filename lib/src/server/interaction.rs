use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "action", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "server.ts")]
pub enum ServerInteractionEvent {
  // legacy interactions - ie stock app only
  __LegacySpotifyPermissions,
}
