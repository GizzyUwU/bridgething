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
pub struct LibraryFavoritesListReply {
  pub page: FavoritesPage,
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
pub struct FavoriteChanged {
  pub uri: String,
  pub liked: bool,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::BridgeToClientMsgData)]
pub enum BridgeToClientLibraryMsg {
  #[bridge_response]
  BrowseReply(LibraryBrowseReply),
  #[bridge_response]
  SearchReply(LibrarySearchReply),
  #[bridge_response]
  RecommendationsReply(LibraryRecommendationsReply),
  #[bridge_response]
  FavoritesListReply(LibraryFavoritesListReply),
  #[bridge_response]
  LibraryErrorReply(LibraryErrorReply),
  #[bridge_event]
  FavoriteChanged(FavoriteChanged),
}
