use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{BrowseResult, FavoritesPage, LibraryError, RecommendationsResult, SearchResult};

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct LibraryBrowseReply {
  pub result: BrowseResult,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct LibrarySearchReply {
  pub result: SearchResult,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct LibraryRecommendationsReply {
  pub result: RecommendationsResult,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Reply to `resolveContext`. Every field is best-effort: a gateway that recognises the uri but
/// cannot cheaply name it answers with `None`s rather than failing the request.
pub struct LibraryResolveContextReply {
  pub name: Option<String>,
  pub artwork_id: Option<String>,
  pub subtitle: Option<String>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct LibraryFavoritesListReply {
  pub page: FavoritesPage,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct LibraryFavoritesContainsReply {
  /// Index-aligned with the request's `uris`.
  pub liked: Vec<bool>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct LibraryErrorReply {
  pub error: LibraryError,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Event: a library item's favorited status changed. Fired for changes made through this
/// client's own `favoritesToggle`/`favoritesSet`/`favoritesSetMany` calls as well as changes
/// made natively on the connected phone (e.g. in the Spotify or Apple Music app).
pub struct FavoriteChanged {
  pub uri: String,
  pub liked: bool,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::BridgeToClientMsgData)]
/// Daemon -> webapp replies and events for the library surface. `BrowseReply`, `SearchReply`,
/// `RecommendationsReply`, `ResolveContextReply`, `FavoritesListReply`, and
/// `FavoritesContainsReply` answer the
/// matching `ClientToBridgeLibraryMsg` request; `LibraryErrorReply` replaces the reply on
/// failure. `FavoriteChanged` is a live event broadcast to every connected webapp whenever a
/// favorite's status changes.
pub enum BridgeToClientLibraryMsg {
  #[bridge_response]
  BrowseReply(LibraryBrowseReply),
  #[bridge_response]
  SearchReply(LibrarySearchReply),
  #[bridge_response]
  RecommendationsReply(LibraryRecommendationsReply),
  #[bridge_response]
  ResolveContextReply(LibraryResolveContextReply),
  #[bridge_response]
  FavoritesListReply(LibraryFavoritesListReply),
  #[bridge_response]
  FavoritesContainsReply(LibraryFavoritesContainsReply),
  #[bridge_response]
  ErrorReply(LibraryErrorReply),
  #[bridge_event]
  FavoriteChanged(FavoriteChanged),
  #[bridge_event]
  ErrorEvent(LibraryErrorReply),
}
