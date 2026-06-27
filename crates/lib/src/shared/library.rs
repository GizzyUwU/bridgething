//! Library surface - typed shapes for browse/search/recommendations
//! results, favorites, and queue items. Per-platform extras don't surface;
//! gateways translate Spotify/Apple Music/MediaSession into these.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

use super::{Album, Artist, Track};

/// Coarse type tag a webapp uses to filter or branch. Mirrors the variant
/// names of `LibraryItem`; kept separate so search/recommendations can
/// constrain by kind without having to construct a sample item.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum ItemKind {
  Track,
  Album,
  Playlist,
  PodcastEpisode,
  Show,
  Artist,
  Station,
}

/// Stable URI + kind a webapp passes back to act on a library item
/// (e.g. `player.play({ uri })`, `library.favorites.toggle({ item })`).
/// `persistent_id` is the platform-stable id when the gateway has one;
/// webapps treat it as opaque.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct ItemRef {
  pub uri: String,
  pub kind: ItemKind,
  pub persistent_id: Option<String>,
}

/// Lean cross-platform shape for a playlist. `uri` is what `player.play`
/// would route on; `track_count` is best-effort (some sources don't expose
/// it cheaply); `owner_name` is whatever the source surfaces (Spotify
/// owner, Apple Music curator, etc.).
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct Playlist {
  pub uri: String,
  pub name: String,
  pub owner_name: Option<String>,
  pub track_count: Option<u32>,
  pub artwork_id: Option<String>,
}

/// One episode of a podcast. `show_name` mirrors what the gateway exposes
/// at episode-level so a webapp can render show + episode without a
/// separate fetch. `published_at_ms` is best-effort; not every gateway
/// surfaces it.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct PodcastEpisode {
  pub uri: String,
  pub name: String,
  pub show_name: Option<String>,
  pub duration_ms: Option<u32>,
  pub published_at_unix_s: Option<u32>,
  pub artwork_id: Option<String>,
}

/// One podcast show (parent of `PodcastEpisode`). `episode_count` is
/// best-effort.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct Show {
  pub uri: String,
  pub name: String,
  pub publisher: Option<String>,
  pub episode_count: Option<u32>,
  pub artwork_id: Option<String>,
}

/// Algorithmic / radio station. `seed` is the URI the station was seeded
/// from when known (artist, track, etc.).
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct Station {
  pub uri: String,
  pub name: String,
  pub seed: Option<String>,
  pub artwork_id: Option<String>,
}

/// One playable / browsable item from the library. Lean per-variant
/// payload - gateways translate platform-specific extras down to these
/// fields, rare per-platform fields just don't surface. Forward-compat:
/// adding new variants or fields is an additive change webapps can
/// branch on.
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum LibraryItem {
  Track(Track),
  Album(Album),
  Playlist(Playlist),
  PodcastEpisode(PodcastEpisode),
  Show(Show),
  Artist(Artist),
  Station(Station),
}

/// Wire-contract node id for the "recently played" browse shelf. A gateway that surfaces a
/// recently-played folder in its root browse gives it this id so the daemon can overlay its own
/// live recently-played history onto that shelf without a refetch; other folders use opaque ids.
pub const RECENTS_NODE_ID: &str = "recently-played";

/// One row in a `BrowseResult`: either a folder (drilldown) or a leaf
/// item the user can play / queue / favorite.
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum BrowseEntry {
  Folder(BrowseFolder),
  Item(LibraryItem),
}

/// A drilldown node. `node_id` is opaque and gateway-defined; webapps
/// pass it back as the next `browse({ node_id })` to descend. `total` is
/// the count of children behind this folder when the gateway can cheaply
/// expose it. `preview_children` is an inline first-N slice of those
/// children so home-shelf shapes don't need a separate drill round-trip;
/// gateways populate it when cheap (Spotify Web API home shelves include
/// previews; Apple Music curated rails do too) and leave it `None`
/// otherwise.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct BrowseFolder {
  pub node_id: String,
  pub title: String,
  pub subtitle: Option<String>,
  pub artwork_id: Option<String>,
  pub total: Option<u32>,
  pub preview_children: Option<Vec<BrowseEntry>>,
}

/// Page of browse results. `total` is the count of items in the
/// underlying collection when the gateway can cheaply expose it (None
/// means indeterminate). `has_more` is the authoritative end-of-data
/// signal - webapps paginate by raising `offset` until `has_more` is
/// false rather than relying on `total`.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct BrowseResult {
  pub entries: Vec<BrowseEntry>,
  pub total: Option<u32>,
  pub has_more: bool,
}

/// Page of search results. `kinds` is the constrained kinds the search
/// honored (echoed back so webapps can detect ignored constraints); items
/// are ranked best-first.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct SearchResult {
  pub items: Vec<LibraryItem>,
  pub kinds: Vec<ItemKind>,
  pub total: Option<u32>,
  pub has_more: bool,
}

/// Page of recommendation results. Gateway decides how seed + kind
/// interact (Spotify uses radio-style seeding, Apple Music uses curated
/// rails) - the daemon doesn't prescribe.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct RecommendationsResult {
  pub items: Vec<LibraryItem>,
  pub total: Option<u32>,
  pub has_more: bool,
}

/// Page of the user's favorited / liked / saved library items. Mixed-kind
/// because most platforms expose one "Saved" surface across kinds.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct FavoritesPage {
  pub items: Vec<LibraryItem>,
  pub total: Option<u32>,
  pub has_more: bool,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum LibraryError {
  /// The named uri or node id does not exist in the gateway's library.
  NotFound { uri: String },
  /// The operation isn't supported by the underlying source (e.g. a
  /// platform that exposes browse but not recommendations).
  NotSupported { reason: String },
  /// User account / OAuth scope does not permit the operation.
  Unauthorized,
  /// No companion is connected to back the surface.
  NoGateway,
}
