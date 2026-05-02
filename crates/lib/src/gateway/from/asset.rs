use bridgething_macros::BridgeEnum;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

use crate::{AssetRetention, gateway::AssetRequest, impl_bridge_request};

#[typeshare]
#[serde_with::serde_as]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct AssetPush {
  pub id: String,
  #[debug(skip)]
  #[serde_as(as = "serde_with::Bytes")]
  #[ts(type = "Uint8Array")]
  pub bytes: Vec<u8>,
  pub mime: Option<String>,
  pub retention: AssetRetention,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct AssetClear {
  pub id: String,
}

/// Typed response payload for an `AssetRequest`. Mirrors `AssetPush`
/// without the retention hint — the daemon picks retention for assets
/// it asked for, since the lifecycle is request-scoped rather than
/// companion-managed.
///
/// Distinct from `server::AssetGot` (webapp protocol, uses payload-id
/// correlation rather than meta correlation).
#[typeshare]
#[serde_with::serde_as]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct AssetGotReply {
  pub id: String,
  #[debug(skip)]
  #[serde_as(as = "serde_with::Bytes")]
  #[ts(type = "Uint8Array")]
  pub bytes: Vec<u8>,
  pub mime: Option<String>,
}

/// Domain error response for an `AssetRequest`: the companion does not
/// have the requested asset.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct AssetNotFoundReply {
  pub id: String,
}

/// Companion-side asset operations:
/// - `Push` (event): proactive load into the daemon cache.
/// - `Clear` (event): drop a previously pushed asset.
/// - `Got` (response): typed reply to a daemon `AssetRequest`.
/// - `NotFound` (response): typed domain error for `AssetRequest` when
///   the companion does not have the asset.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum GatewayToBridgeAssetMsg {
  Push(AssetPush),
  Clear(AssetClear),
  #[bridge_response]
  Got(AssetGotReply),
  #[bridge_response]
  NotFound(AssetNotFoundReply),
}

impl_bridge_request! {
  request: AssetRequest,
  surface: Asset,
  request_variant: Request(_),
  response: AssetGotReply,
  response_variant: Got(_),
  error: AssetNotFoundReply,
  error_variant: NotFound(_),
}
