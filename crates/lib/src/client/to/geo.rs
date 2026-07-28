use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{GeoError, Position};

/// Watch handle. Webapps pass the token back as
/// `ClientToBridgeGeoMsg::Unwatch { token }` to release the watch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct GeoWatchReply {
  pub token: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Response to `geo.getOnce`.
pub struct GeoGetOnceReply {
  pub position: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Error response to a failed `geo.watch` or `geo.getOnce` request.
pub struct GeoErrorReply {
  pub error: GeoError,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::BridgeToClientMsgData)]
/// Daemon -> webapp location surface: the `Position` stream a watch
/// produces, plus replies to `geo.watch` and `geo.getOnce`.
pub enum BridgeToClientGeoMsg {
  #[bridge_event]
  Position(Position),
  #[bridge_event]
  ErrorEvent(GeoErrorReply),
  #[bridge_response]
  WatchReply(GeoWatchReply),
  #[bridge_response]
  GetOnceReply(GeoGetOnceReply),
  #[bridge_response]
  ErrorReply(GeoErrorReply),
}
