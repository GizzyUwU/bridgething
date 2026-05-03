use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{GeoError, Position};

/// Watch handle. Webapps pass the token back as
/// `ClientToBridgeGeoMsg::Unwatch { token }` to release the watch.
/// Tokens are scoped to the webapp's WS connection — the daemon
/// auto-releases on disconnect.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct GeoWatchReply {
  pub token: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct GeoGetOnceReply {
  pub position: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct GeoErrorReply {
  pub error: GeoError,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::BridgeToClientMsgData)]
pub enum BridgeToClientGeoMsg {
  #[bridge_event]
  Position(Position),
  #[bridge_response]
  WatchReply(GeoWatchReply),
  #[bridge_response]
  GetOnceReply(GeoGetOnceReply),
  #[bridge_response]
  ErrorReply(GeoErrorReply),
}
