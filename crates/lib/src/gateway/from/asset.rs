use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

use super::transfer::TransferBody;

/// Invalidate the daemon-side cached asset for `id`. The companion's
/// escape hatch when it knows an asset it previously served is stale.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct AssetClear {
  pub id: String,
}

/// Typed terminal response for an `AssetRequest`. Small assets arrive
/// inline; larger ones declare a stream whose ref id is the originating
/// request id, with the bytes following as `TransferFragment` events on
/// the bulk lane (so now-playing traffic preempts them between
/// fragments).
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct AssetGotReply {
  pub id: String,
  pub mime: Option<String>,
  pub body: TransferBody,
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

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::GatewayToBridgeMsgData)]
pub enum GatewayToBridgeAssetMsg {
  #[bridge_event]
  Clear(AssetClear),
  #[bridge_response]
  Got(AssetGotReply),
  #[bridge_response]
  NotFound(AssetNotFoundReply),
}
