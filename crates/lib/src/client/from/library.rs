use bridgething_macros::{BridgeDispatch, BridgeEnum, WireRequest};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{ItemKind, ItemRef};

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[wire_request(
  direction = ClientToBridge,
  surface = Library,
  request_variant = Browse,
  response = crate::client::LibraryBrowseReply,
  response_variant = BrowseReply,
  error = crate::client::LibraryErrorReply,
  error_variant = LibraryErrorReply,
)]
pub struct LibraryBrowse {
  pub node_id: Option<String>,
  pub limit: u32,
  pub offset: u32,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[wire_request(
  direction = ClientToBridge,
  surface = Library,
  request_variant = Search,
  response = crate::client::LibrarySearchReply,
  response_variant = SearchReply,
  error = crate::client::LibraryErrorReply,
  error_variant = LibraryErrorReply,
)]
pub struct LibrarySearch {
  pub query: String,
  pub kinds: Option<Vec<ItemKind>>,
  pub limit: u32,
  pub offset: u32,
}

/// Recommendations seeded by up to 5 items. Spotify hard-caps at 5
/// combined seeds across tracks/artists/genres.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[wire_request(
  direction = ClientToBridge,
  surface = Library,
  request_variant = Recommendations,
  response = crate::client::LibraryRecommendationsReply,
  response_variant = RecommendationsReply,
  error = crate::client::LibraryErrorReply,
  error_variant = LibraryErrorReply,
)]
pub struct LibraryRecommendations {
  pub seeds: Vec<ItemRef>,
  pub kind: Option<ItemKind>,
  pub limit: u32,
  pub offset: u32,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[wire_request(
  direction = ClientToBridge,
  surface = Library,
  request_variant = FavoritesList,
  response = crate::client::LibraryFavoritesListReply,
  response_variant = FavoritesListReply,
  error = crate::client::LibraryErrorReply,
  error_variant = LibraryErrorReply,
)]
pub struct LibraryFavoritesList {
  pub limit: u32,
  pub offset: u32,
}

/// Batch "is each of these favorited?" lookup. Reply `liked` is
/// index-aligned with the request `uris`, capped at 50 per call.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[wire_request(
  direction = ClientToBridge,
  surface = Library,
  request_variant = FavoritesContains,
  response = crate::client::LibraryFavoritesContainsReply,
  response_variant = FavoritesContainsReply,
  error = crate::client::LibraryErrorReply,
  error_variant = LibraryErrorReply,
)]
pub struct LibraryFavoritesContains {
  pub uris: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct FavoritesToggle {
  pub item: ItemRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct FavoritesSet {
  pub item: ItemRef,
  pub liked: bool,
}

/// Bulk favorites mutation. Webapps observing partial success listen
/// for `FavoriteChanged` events.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct FavoritesSetMany {
  pub entries: Vec<FavoritesSet>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum, BridgeDispatch)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::ClientToBridgeMsgData)]
pub enum ClientToBridgeLibraryMsg {
  #[bridge_request]
  Browse(LibraryBrowse),
  #[bridge_request]
  Search(LibrarySearch),
  #[bridge_request]
  Recommendations(LibraryRecommendations),
  #[bridge_request]
  FavoritesList(LibraryFavoritesList),
  #[bridge_request]
  FavoritesContains(LibraryFavoritesContains),
  #[bridge_command]
  FavoritesToggle(FavoritesToggle),
  #[bridge_command]
  FavoritesSet(FavoritesSet),
  #[bridge_command]
  FavoritesSetMany(FavoritesSetMany),
}
