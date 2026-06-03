use bridgething_macros::{BridgeEnum, WireRequest};
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

use crate::{ItemKind, ItemRef};

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = BridgeToGateway,
  surface = Library,
  request_variant = Browse,
  response = crate::gateway::BrowseReply,
  response_variant = BrowseReply,
  error = crate::gateway::LibraryErrorReply,
  error_variant = LibraryErrorReply,
)]
pub struct LibraryBrowseRequest {
  /// Drilldown node id from a prior `BrowseFolder`. `None` means "root".
  pub node_id: Option<String>,
  pub limit: u32,
  pub offset: u32,
}

/// Resolve a single context uri (playlist / album / show / artist) to its
/// name + cover art. Used to populate a stock preset slot the device only
/// knows by `context_uri`.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = BridgeToGateway,
  surface = Library,
  request_variant = ResolveContext,
  response = crate::gateway::ContextResolveReply,
  response_variant = ContextResolveReply,
  error = crate::gateway::LibraryErrorReply,
  error_variant = LibraryErrorReply,
)]
pub struct LibraryResolveContextRequest {
  pub uri: String,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = BridgeToGateway,
  surface = Library,
  request_variant = Search,
  response = crate::gateway::SearchReply,
  response_variant = SearchReply,
  error = crate::gateway::LibraryErrorReply,
  error_variant = LibraryErrorReply,
)]
pub struct LibrarySearchRequest {
  pub query: String,
  pub kinds: Option<Vec<ItemKind>>,
  pub limit: u32,
  pub offset: u32,
}

/// Recommendations seeded by up to 5 items. The daemon caps the seed
/// list at the platform-permissive limit (Spotify hard-caps at 5
/// combined seeds across tracks/artists/genres). Gateway decides how to
/// distribute seeds across its native API surfaces.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = BridgeToGateway,
  surface = Library,
  request_variant = Recommendations,
  response = crate::gateway::RecommendationsReply,
  response_variant = RecommendationsReply,
  error = crate::gateway::LibraryErrorReply,
  error_variant = LibraryErrorReply,
)]
pub struct LibraryRecommendationsRequest {
  pub seeds: Vec<ItemRef>,
  pub kind: Option<ItemKind>,
  pub limit: u32,
  pub offset: u32,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = BridgeToGateway,
  surface = Library,
  request_variant = FavoritesList,
  response = crate::gateway::FavoritesListReply,
  response_variant = FavoritesListReply,
  error = crate::gateway::LibraryErrorReply,
  error_variant = LibraryErrorReply,
)]
pub struct LibraryFavoritesListRequest {
  pub limit: u32,
  pub offset: u32,
}

/// Batch "is each of these favorited?" lookup. Mirrors Spotify's
/// `GET /me/tracks/contains` shape. Reply `liked` is index-aligned with
/// the request `uris`.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = BridgeToGateway,
  surface = Library,
  request_variant = FavoritesContains,
  response = crate::gateway::FavoritesContainsReply,
  response_variant = FavoritesContainsReply,
  error = crate::gateway::LibraryErrorReply,
  error_variant = LibraryErrorReply,
)]
pub struct LibraryFavoritesContainsRequest {
  pub uris: Vec<String>,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct FavoritesToggle {
  pub item: ItemRef,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct FavoritesSet {
  pub item: ItemRef,
  pub liked: bool,
}

/// Bulk favorites mutation. `entries` are independent `FavoritesSet`
/// applications; gateway returns once it has issued each underlying
/// platform call. Per-entry errors are not surfaced - companion logs
/// and best-efforts the rest. Webapps observing partial success listen
/// for `FavoriteChanged` events.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct FavoritesSetMany {
  pub entries: Vec<FavoritesSet>,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::BridgeToGatewayMsgData)]
pub enum BridgeToGatewayLibraryMsg {
  #[bridge_request]
  Browse(LibraryBrowseRequest),
  #[bridge_request]
  ResolveContext(LibraryResolveContextRequest),
  #[bridge_request]
  Search(LibrarySearchRequest),
  #[bridge_request]
  Recommendations(LibraryRecommendationsRequest),
  #[bridge_request]
  FavoritesList(LibraryFavoritesListRequest),
  #[bridge_request]
  FavoritesContains(LibraryFavoritesContainsRequest),
  #[bridge_command]
  FavoritesToggle(FavoritesToggle),
  #[bridge_command]
  FavoritesSet(FavoritesSet),
  #[bridge_command]
  FavoritesSetMany(FavoritesSetMany),
}
