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
  pub page_token: Option<String>,
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
  pub page_token: Option<String>,
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
  pub seed: Option<ItemRef>,
  pub kind: Option<ItemKind>,
  pub limit: Option<u32>,
  pub page_token: Option<String>,
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
  pub page_token: Option<String>,
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::BridgeToGatewayMsgData)]
pub enum BridgeToGatewayLibraryMsg {
  #[bridge_request]
  Browse(LibraryBrowseRequest),
  #[bridge_request]
  Search(LibrarySearchRequest),
  #[bridge_request]
  Recommendations(LibraryRecommendationsRequest),
  #[bridge_request]
  FavoritesList(LibraryFavoritesListRequest),
  #[bridge_command]
  FavoritesToggle(FavoritesToggle),
  #[bridge_command]
  FavoritesSet(FavoritesSet),
}
