use bridgething_macros::BridgeEnum;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

/// Companion ask the daemon to apply a previously-pushed `.swu` from the
/// asset cache. The companion fetches the manifest from its update server,
/// downloads the artifact, pushes the bytes via `AssetPush` with retention
/// `Ttl(2h)`, then sends `ApplyUpdate` referencing the same `asset_id`.
///
/// The daemon verifies size + sha256 against the cached blob before handing
/// it to swupdate. `manifest_url` is recorded for tracing only.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct ApplyUpdate {
  pub asset_id: String,
  pub manifest_url: Option<String>,
  pub expected_sha256: String,
  pub expected_size: u32,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::GatewayToBridgeMsgData)]
pub enum GatewayToBridgeSystemMsg {
  #[bridge_command]
  ApplyUpdate(ApplyUpdate),
  #[bridge_command]
  CancelUpdate,
}
