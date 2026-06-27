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

/// Resolved metadata for a single context uri (playlist / album / show /
/// artist), used to populate a stock preset's name + cover art.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct ContextResolveReply {
  pub name: Option<String>,
  pub artwork_id: Option<String>,
  pub subtitle: Option<String>,
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

/// Batch favorites-contains reply. `liked` is index-aligned with the
/// request's `uris`.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct FavoritesContainsReply {
  pub liked: Vec<bool>,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct LibraryErrorReply {
  pub error: LibraryError,
}

/// Fired when the favorited / liked status of an item changes -
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

/// Which slice of the user's library changed, so a consumer can scope a
/// refetch. The daemon invalidates its home cache on any scope; the
/// distinction is informational for richer webapp consumers.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum LibraryScope {
  Saved,
  Playlists,
}

/// Fired when the user mutates their library on the gateway-side app while
/// connected (a like, a playlist edit) and the change did NOT originate from a
/// daemon command - so the daemon must invalidate any cached browse / home view.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct LibraryChanged {
  pub scope: LibraryScope,
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
  ContextResolveReply(ContextResolveReply),
  #[bridge_response]
  SearchReply(SearchReply),
  #[bridge_response]
  RecommendationsReply(RecommendationsReply),
  #[bridge_response]
  FavoritesListReply(FavoritesListReply),
  #[bridge_response]
  FavoritesContainsReply(FavoritesContainsReply),
  #[bridge_response]
  LibraryErrorReply(LibraryErrorReply),
  #[bridge_event]
  FavoriteChanged(FavoriteChanged),
  #[bridge_event]
  LibraryChanged(LibraryChanged),
}
