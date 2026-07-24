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
  pub node_id: Option<String>,
  pub limit: u32,
  pub offset: u32,
  #[serde(default)]
  pub sections: Option<u32>,
  #[serde(default)]
  pub preview: Option<u32>,
}

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
