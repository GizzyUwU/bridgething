use bridgething_macros::{BridgeEnum, WireRequest};
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct TrackIdentity {
  pub artist: String,
  pub track: String,
  pub album: Option<String>,
  pub duration_ms: Option<u32>,
  pub isrc: Option<String>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = BridgeToGateway,
  surface = Lyrics,
  request_variant = Get,
  response = crate::gateway::LyricsReply,
  response_variant = LyricsReply,
  error = crate::gateway::LyricsErrorReply,
  error_variant = LyricsErrorReply,
)]
pub struct LyricsRequest {
  pub track: TrackIdentity,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::BridgeToGatewayMsgData)]
pub enum BridgeToGatewayLyricsMsg {
  #[bridge_request]
  Get(LyricsRequest),
}
