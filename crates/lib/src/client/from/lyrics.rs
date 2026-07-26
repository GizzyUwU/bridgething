use bridgething_macros::{BridgeDispatch, BridgeEnum, WireRequest};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Webapp asks for lyrics of whatever is playing right now. Deliberately payload-free: the
/// daemon builds the track identity from its own player state, so a webapp cannot disagree with
/// it about what is playing.
#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = ClientToBridge,
  surface = Lyrics,
  request_variant = Get,
  response = crate::client::LyricsReply,
  response_variant = LyricsReply,
  error = crate::client::LyricsErrorReply,
  error_variant = LyricsErrorReply,
)]
pub struct LyricsGet;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum, BridgeDispatch)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::ClientToBridgeMsgData)]
/// Webapp -> daemon lyrics surface. Request/reply; check `Capabilities::available.lyrics`
/// before offering a lyrics view, since a gateway that cannot supply them answers with an
/// error rather than empty lyrics.
pub enum ClientToBridgeLyricsMsg {
  #[bridge_request]
  Get,
}
