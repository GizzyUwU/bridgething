use bridgething_macros::{BridgeEnum, WireRequest};
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
  pub page_token: Option<String>,
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
  pub page_token: Option<String>,
}

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
  pub seed: Option<ItemRef>,
  pub kind: Option<ItemKind>,
  pub limit: Option<u32>,
  pub page_token: Option<String>,
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
  pub page_token: Option<String>,
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

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
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
  #[bridge_command]
  FavoritesToggle(FavoritesToggle),
  #[bridge_command]
  FavoritesSet(FavoritesSet),
}
