use bridgething_macros::BridgeEnum;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

use crate::{BrowseResult, FavoritesPage, LibraryError, RecommendationsResult, SearchResult};

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct BrowseReply {
  pub result: BrowseResult,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct SearchReply {
  pub result: SearchResult,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct RecommendationsReply {
  pub result: RecommendationsResult,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct FavoritesListReply {
  pub page: FavoritesPage,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct LibraryErrorReply {
  pub error: LibraryError,
}

/// Fired when the favorited / liked status of an item changes —
/// regardless of whether it was driven by the daemon (FavoritesToggle/Set
/// command) or by the user mutating it on the gateway-side app directly.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct FavoriteChanged {
  pub uri: String,
  pub liked: bool,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::GatewayToBridgeMsgData)]
pub enum GatewayToBridgeLibraryMsg {
  #[bridge_response]
  BrowseReply(BrowseReply),
  #[bridge_response]
  SearchReply(SearchReply),
  #[bridge_response]
  RecommendationsReply(RecommendationsReply),
  #[bridge_response]
  FavoritesListReply(FavoritesListReply),
  #[bridge_response]
  LibraryErrorReply(LibraryErrorReply),
  #[bridge_event]
  FavoriteChanged(FavoriteChanged),
}
