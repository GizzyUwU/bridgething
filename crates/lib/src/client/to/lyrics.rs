use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Lyrics;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Successful reply to `LyricsGet`. `lyrics` is `None` when the lookup succeeded but the
/// provider has nothing for this track, which is a normal answer rather than an error.
pub struct LyricsReply {
  /// Uri of the track these lyrics were resolved for. The request carries no track, so a
  /// webapp compares this against its own now-playing state to discard a reply that landed
  /// after the track moved on. `None` when the playing item carries no uri.
  pub track_uri: Option<String>,
  /// Stable per-item id for the same comparison when there is no uri, as on iAP2 phone media.
  pub track_persistent_id: Option<String>,
  pub lyrics: Option<Lyrics>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Why a lyrics lookup could not be answered. Only `LookupFailed` is worth retrying for the
/// same track; the rest tell a webapp to stop asking until something changes.
pub enum LyricsError {
  /// No companion is connected to back the surface.
  NoGateway,
  /// A companion is connected but supplies no lyrics, so no track will ever resolve.
  NotSupported,
  /// Nothing is playing, so there is no track to look up.
  NothingPlaying,
  /// The playing item lacks the artist or title needed to identify it.
  TrackUnidentifiable,
  /// The gateway was asked and the lookup failed. Retrying may succeed.
  LookupFailed { reason: String },
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Domain-error reply to `LyricsGet`.
pub struct LyricsErrorReply {
  pub error: LyricsError,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::BridgeToClientMsgData)]
/// Daemon -> webapp lyrics replies.
pub enum BridgeToClientLyricsMsg {
  #[bridge_response]
  LyricsReply(LyricsReply),
  #[bridge_response]
  LyricsErrorReply(LyricsErrorReply),
}
