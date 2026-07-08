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
/// Payload for the `browse` request: page through a folder in the library tree, or the root
/// menu when `node_id` is `None`. Root-level results are cached by the daemon for up to 5
/// minutes, so a fresh call immediately after a library change on the gateway side may still
/// return the previous shelf layout.
pub struct LibraryBrowse {
  /// Folder to descend into, from a prior result's `BrowseFolder::node_id`. `None` browses the root.
  pub node_id: Option<String>,
  /// Requested page size; the daemon clamps this to 100 regardless of the value sent.
  pub limit: u32,
  pub offset: u32,
  /// Root only: cap on the number of folders returned. `None` returns every folder.
  #[serde(default)]
  pub sections: Option<u32>,
  /// Root only: preview children per folder. `None` is the gateway default; `0` skips preview
  /// hydration entirely and returns a cheap index of node ids, titles, and totals.
  #[serde(default)]
  pub preview: Option<u32>,
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
/// Payload for the `search` request: free-text search across the connected gateway's library.
pub struct LibrarySearch {
  pub query: String,
  /// Restrict results to these item kinds. `None` searches every kind.
  pub kinds: Option<Vec<ItemKind>>,
  /// Requested page size; the daemon clamps this to 100 regardless of the value sent.
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
  /// Seed items; the daemon truncates this to the first 5 regardless of the count sent.
  pub seeds: Vec<ItemRef>,
  /// Restrict results to this item kind. `None` lets the gateway choose based on the seeds.
  pub kind: Option<ItemKind>,
  /// Requested page size; the daemon clamps this to 100 regardless of the value sent.
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
/// Payload for the `favoritesList` request: page through the user's saved/liked library items,
/// mixed across kinds.
pub struct LibraryFavoritesList {
  /// Requested page size; the daemon clamps this to 100 regardless of the value sent.
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
  /// Uris to check; the daemon truncates this to the first 50 regardless of the count sent.
  pub uris: Vec<String>,
}

/// Payload for the `favoritesToggle` command: flip `item`'s favorited state (liked becomes
/// unliked and vice versa). Fire-and-forget; the result surfaces as a `FavoriteChanged` event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct FavoritesToggle {
  pub item: ItemRef,
}

/// Payload for the `favoritesSet` command: set `item`'s favorited state explicitly. Fire-and-
/// forget; the result surfaces as a `FavoriteChanged` event.
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
/// Webapp -> daemon library surface: browse/search/recommend across the connected gateway's
/// library, and read or mutate favorites. `Browse`, `Search`, `Recommendations`,
/// `FavoritesList`, and `FavoritesContains` are request/reply and fail with
/// `LibraryErrorReply` when no gateway is connected. `FavoritesToggle`, `FavoritesSet`, and
/// `FavoritesSetMany` are fire-and-forget commands with no completion reply; if no gateway is
/// connected they are silently dropped, and on success their effect surfaces later as a
/// `FavoriteChanged` event.
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
