use bridgething_macros::BridgeEnum;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

use crate::CompanionAuthorityScope;

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct AuthorityClaim {
  pub scope: CompanionAuthorityScope,
}

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct AuthorityRelease {
  pub scope: CompanionAuthorityScope,
}

/// Companion declares per-scope authority. `Claim` is idempotent and may
/// be re-issued to refresh the freshness timestamp. `Release` is the
/// "stop preferring my data for this scope" signal. Stale claims fall
/// back automatically after `AUTHORITY_STALE_TIMEOUT_SECS` (default 5).
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::GatewayToBridgeMsgData)]
pub enum GatewayToBridgeAuthorityMsg {
  #[bridge_event]
  Claim(AuthorityClaim),
  #[bridge_event]
  Release(AuthorityRelease),
}
