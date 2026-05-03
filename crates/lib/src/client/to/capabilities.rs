use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Capabilities;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct CapabilitiesSnapshot {
  pub capabilities: Capabilities,
}

/// Daemon → webapp capabilities surface. `Update` is the broadcast event
/// fired on connect + on every change; `Snapshot` is the typed reply to
/// `CapabilitiesGet`. Webapps that auto-react to capability change
/// listen on `Update` and don't need to call `Get`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::BridgeToClientMsgData)]
pub enum BridgeToClientCapabilitiesMsg {
  #[bridge_event]
  Update(CapabilitiesSnapshot),
  #[bridge_response]
  Snapshot(CapabilitiesSnapshot),
}
