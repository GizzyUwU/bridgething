use std::{collections::HashSet, time::Duration};

use libbridgething::{
  Album, Artist, BrowseEntry, BrowseResult, LibraryError, LibraryItem, QueueItem, RECENTS_NODE_ID, Track,
  client::{
    BridgeToClientMsgData, ClientToBridgeLibraryMsgDispatch, FavoritesSet as ClientFavoritesSet,
    FavoritesSetMany as ClientFavoritesSetMany, FavoritesToggle as ClientFavoritesToggle, LibraryBrowse,
    LibraryBrowseReply, LibraryErrorReply, LibraryFavoritesContains, LibraryFavoritesContainsReply,
    LibraryFavoritesList, LibraryFavoritesListReply, LibraryRecommendations, LibraryRecommendationsReply,
    LibrarySearch, LibrarySearchReply,
  },
  gateway::{
    self, BridgeToGatewayLibraryMsgCommand, BrowseReply, FavoritesContainsReply, LibraryBrowseRequest,
    LibraryFavoritesContainsRequest, LibraryFavoritesListRequest, LibraryRecommendationsRequest, LibrarySearchRequest,
  },
  wire::{RequestError, WireRequest},
};

use super::{HandlerResult, MsgHandle};
use crate::{
  bluetooth::GatewayMan,
  player::{Player, is_synthetic_uri},
  state::{BrowseContentCache, RootBrowseCache},
};

const RECOMMENDATIONS_SEEDS_MAX: usize = 5;
const FAVORITES_CONTAINS_MAX: usize = 50;
const BROWSE_LIMIT_MAX: u32 = 100;
const ROOT_BROWSE_TTL: Duration = Duration::from_secs(300);

pub struct LibraryHandler {
  handle: MsgHandle,
}

impl LibraryHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  fn has_gateway(&self) -> bool {
    self.handle.state.capabilities.snapshot().gateway.is_some()
  }

  async fn forward_command(&self, cmd: BridgeToGatewayLibraryMsgCommand) -> HandlerResult {
    self.handle.bluetooth.gateway_man.broadcast_command(cmd).await;
    Ok(())
  }

  async fn respond_error<R>(&self, error: LibraryError) -> HandlerResult
  where
    R: WireRequest<Inbound = BridgeToClientMsgData, DomainError = LibraryErrorReply>,
  {
    self
      .handle
      .respond_err::<R>(LibraryErrorReply { error })
      .await
      .map_err(Into::into)
  }

  async fn respond_request_error<R>(&self, verb: &str, err: RequestError<gateway::LibraryErrorReply>) -> HandlerResult
  where
    R: WireRequest<Inbound = BridgeToClientMsgData, DomainError = LibraryErrorReply>,
  {
    let error = match err {
      RequestError::Domain(domain) => domain.error,
      RequestError::Protocol(err) => {
        tracing::warn!(?err, "{verb} protocol error");
        LibraryError::NotSupported {
          reason: format!("{err:?}"),
        }
      }
      RequestError::ResponseMismatch => {
        tracing::error!("{verb} response did not match expected shape");
        LibraryError::NotSupported {
          reason: "response shape mismatch".into(),
        }
      }
    };
    self.respond_error::<R>(error).await
  }
}

impl ClientToBridgeLibraryMsgDispatch for LibraryHandler {
  type Output = HandlerResult;

  async fn browse(&self, params: LibraryBrowse) -> HandlerResult {
    let LibraryBrowse { node_id, limit, offset } = params;
    if !self.has_gateway() {
      return self.respond_error::<LibraryBrowse>(LibraryError::NoGateway).await;
    }
    let outbound = LibraryBrowseRequest {
      node_id,
      limit: limit.min(BROWSE_LIMIT_MAX),
      offset,
    };
    match browse_request(
      &self.handle.bluetooth.gateway_man,
      &self.handle.state.root_browse,
      &self.handle.state.browse_content,
      &self.handle.state.player,
      outbound,
    )
    .await
    {
      Ok(reply) => {
        self
          .handle
          .respond_to::<LibraryBrowse>(LibraryBrowseReply { result: reply.result })
          .await?;
      }
      Err(err) => {
        self
          .respond_request_error::<LibraryBrowse>("library.browse", err)
          .await?
      }
    }
    Ok(())
  }

  async fn search(&self, params: LibrarySearch) -> HandlerResult {
    let LibrarySearch {
      query,
      kinds,
      limit,
      offset,
    } = params;
    if !self.has_gateway() {
      return self.respond_error::<LibrarySearch>(LibraryError::NoGateway).await;
    }
    let outbound = LibrarySearchRequest {
      query,
      kinds,
      limit: limit.min(BROWSE_LIMIT_MAX),
      offset,
    };
    match self.handle.bluetooth.gateway_man.request(None, outbound).await {
      Ok(reply) => {
        self
          .handle
          .respond_to::<LibrarySearch>(LibrarySearchReply { result: reply.result })
          .await?;
      }
      Err(err) => {
        self
          .respond_request_error::<LibrarySearch>("library.search", err)
          .await?
      }
    }
    Ok(())
  }

  async fn recommendations(&self, params: LibraryRecommendations) -> HandlerResult {
    let req = params;
    if !self.has_gateway() {
      return self
        .respond_error::<LibraryRecommendations>(LibraryError::NoGateway)
        .await;
    }
    let mut seeds = req.seeds;
    seeds.truncate(RECOMMENDATIONS_SEEDS_MAX);
    let outbound = LibraryRecommendationsRequest {
      seeds,
      kind: req.kind,
      limit: req.limit.min(BROWSE_LIMIT_MAX),
      offset: req.offset,
    };
    match self.handle.bluetooth.gateway_man.request(None, outbound).await {
      Ok(reply) => {
        self
          .handle
          .respond_to::<LibraryRecommendations>(LibraryRecommendationsReply { result: reply.result })
          .await?;
      }
      Err(err) => {
        self
          .respond_request_error::<LibraryRecommendations>("library.recommendations", err)
          .await?
      }
    }
    Ok(())
  }

  async fn favorites_list(&self, params: LibraryFavoritesList) -> HandlerResult {
    let LibraryFavoritesList { limit, offset } = params;
    if !self.has_gateway() {
      return self
        .respond_error::<LibraryFavoritesList>(LibraryError::NoGateway)
        .await;
    }
    let outbound = LibraryFavoritesListRequest {
      limit: limit.min(BROWSE_LIMIT_MAX),
      offset,
    };
    match self.handle.bluetooth.gateway_man.request(None, outbound).await {
      Ok(reply) => {
        self
          .handle
          .respond_to::<LibraryFavoritesList>(LibraryFavoritesListReply { page: reply.page })
          .await?;
      }
      Err(err) => {
        self
          .respond_request_error::<LibraryFavoritesList>("library.favoritesList", err)
          .await?
      }
    }
    Ok(())
  }

  async fn favorites_contains(&self, params: LibraryFavoritesContains) -> HandlerResult {
    let LibraryFavoritesContains { uris } = params;
    if !self.has_gateway() {
      return self
        .respond_error::<LibraryFavoritesContains>(LibraryError::NoGateway)
        .await;
    }
    if uris.is_empty() {
      self
        .handle
        .respond_to::<LibraryFavoritesContains>(LibraryFavoritesContainsReply { liked: Vec::new() })
        .await?;
      return Ok(());
    }
    let mut uris = uris;
    uris.truncate(FAVORITES_CONTAINS_MAX);
    let outbound = LibraryFavoritesContainsRequest { uris };
    match favorites_contains_request(&self.handle.bluetooth.gateway_man, outbound).await {
      Ok(reply) => {
        self
          .handle
          .respond_to::<LibraryFavoritesContains>(LibraryFavoritesContainsReply { liked: reply.liked })
          .await?;
      }
      Err(err) => {
        self
          .respond_request_error::<LibraryFavoritesContains>("library.favoritesContains", err)
          .await?
      }
    }
    Ok(())
  }

  async fn favorites_toggle(&self, params: ClientFavoritesToggle) -> HandlerResult {
    self
      .forward_command(BridgeToGatewayLibraryMsgCommand::FavoritesToggle(
        gateway::FavoritesToggle { item: params.item },
      ))
      .await
  }

  async fn favorites_set(&self, params: ClientFavoritesSet) -> HandlerResult {
    self
      .forward_command(BridgeToGatewayLibraryMsgCommand::FavoritesSet(gateway::FavoritesSet {
        item: params.item,
        liked: params.liked,
      }))
      .await
  }

  async fn favorites_set_many(&self, params: ClientFavoritesSetMany) -> HandlerResult {
    let entries = params
      .entries
      .into_iter()
      .map(|e| gateway::FavoritesSet {
        item: e.item,
        liked: e.liked,
      })
      .collect();
    self
      .forward_command(BridgeToGatewayLibraryMsgCommand::FavoritesSetMany(
        gateway::FavoritesSetMany { entries },
      ))
      .await
  }
}

pub(super) async fn browse_request(
  gateway_man: &GatewayMan,
  root_cache: &RootBrowseCache,
  content_cache: &BrowseContentCache,
  player: &Player,
  req: LibraryBrowseRequest,
) -> Result<BrowseReply, RequestError<gateway::LibraryErrorReply>> {
  if req.node_id.as_deref().is_some_and(is_synthetic_uri) {
    return Ok(BrowseReply {
      result: BrowseResult {
        entries: Vec::new(),
        total: None,
        has_more: false,
      },
    });
  }
  let Some(node_id) = req.node_id.clone() else {
    let base = root_cache
      .get_or_fetch(player.root_browse_gen(), ROOT_BROWSE_TTL, || async {
        gateway_man.request(None, req).await.map(|reply| reply.result)
      })
      .await?;
    return Ok(BrowseReply {
      result: overlay_home_recents(base, &player.home_recents()),
    });
  };
  let offset = req.offset;
  let limit = req.limit;
  let result = content_cache
    .get_or_fetch(&node_id, offset, limit, player.root_browse_gen(), || async move {
      gateway_man.request(None, req).await.map(|reply| reply.result)
    })
    .await?;
  Ok(BrowseReply { result })
}

fn overlay_home_recents(mut base: BrowseResult, home_recents: &[QueueItem]) -> BrowseResult {
  if home_recents.is_empty() {
    return base;
  }
  let Some(folder) = base.entries.iter_mut().find_map(|entry| match entry {
    BrowseEntry::Folder(folder) if folder.node_id == RECENTS_NODE_ID => Some(folder),
    _ => None,
  }) else {
    return base;
  };
  let existing = folder.preview_children.take().unwrap_or_default();
  let cap = existing.len().max(home_recents.len());
  let mut seen: HashSet<String> = HashSet::new();
  let mut children: Vec<BrowseEntry> = Vec::with_capacity(cap);
  for item in home_recents {
    if seen.insert(item.uri.clone()) {
      children.push(BrowseEntry::Item(LibraryItem::Track(queue_item_to_track(item))));
    }
  }
  for entry in existing {
    let keep = match track_uri(&entry) {
      Some(uri) => seen.insert(uri.to_string()),
      None => true,
    };
    if keep {
      children.push(entry);
    }
  }
  children.truncate(cap);
  folder.preview_children = Some(children);
  base
}

fn track_uri(entry: &BrowseEntry) -> Option<&str> {
  match entry {
    BrowseEntry::Item(LibraryItem::Track(track)) => Some(track.id.as_str()),
    _ => None,
  }
}

fn queue_item_to_track(item: &QueueItem) -> Track {
  let artist = Artist {
    id: item.artist_uri.clone().unwrap_or_default(),
    name: item.artist.clone().unwrap_or_default(),
    artwork_id: None,
  };
  Track {
    id: item.uri.clone(),
    name: item.title.clone().unwrap_or_default(),
    album: Album {
      id: item.album_uri.clone().unwrap_or_default(),
      name: item.album.clone().unwrap_or_default(),
      artwork_id: None,
    },
    artist: artist.clone(),
    artists: vec![artist],
    duration_ms: item.duration_ms.unwrap_or(0),
    image_id: item.artwork_id.clone().unwrap_or_default(),
    saved: false,
  }
}

pub(super) async fn favorites_contains_request(
  gateway_man: &GatewayMan,
  req: LibraryFavoritesContainsRequest,
) -> Result<FavoritesContainsReply, RequestError<gateway::LibraryErrorReply>> {
  if !req.uris.is_empty() && req.uris.iter().all(|uri| is_synthetic_uri(uri)) {
    return Ok(FavoritesContainsReply {
      liked: vec![false; req.uris.len()],
    });
  }
  gateway_man.request(None, req).await
}

#[cfg(test)]
mod tests {
  use libbridgething::BrowseFolder;

  use super::*;

  fn qi(uri: &str) -> QueueItem {
    QueueItem {
      uri: uri.into(),
      title: Some(format!("title-{uri}")),
      artist: Some("Artist".into()),
      artist_uri: None,
      album: None,
      album_uri: None,
      artwork_id: Some(format!("art-{uri}")),
      duration_ms: Some(1000),
      persistent_id: None,
      queued: false,
    }
  }

  fn track_entry(id: &str) -> BrowseEntry {
    BrowseEntry::Item(LibraryItem::Track(Track {
      id: id.into(),
      name: format!("name-{id}"),
      ..Track::default()
    }))
  }

  fn recents_base(children: Vec<BrowseEntry>) -> BrowseResult {
    BrowseResult {
      entries: vec![BrowseEntry::Folder(BrowseFolder {
        node_id: RECENTS_NODE_ID.into(),
        title: "Recently Played".into(),
        subtitle: None,
        artwork_id: None,
        total: Some(children.len() as u32),
        preview_children: Some(children),
      })],
      total: None,
      has_more: false,
    }
  }

  fn shelf_uris(result: &BrowseResult) -> Vec<String> {
    let BrowseEntry::Folder(folder) = &result.entries[0] else {
      panic!("expected a folder")
    };
    folder
      .preview_children
      .as_ref()
      .unwrap()
      .iter()
      .filter_map(track_uri)
      .map(str::to_string)
      .collect()
  }

  #[test]
  fn empty_home_recents_leaves_base_untouched() {
    let base = recents_base(vec![track_entry("a"), track_entry("b")]);
    assert_eq!(overlay_home_recents(base.clone(), &[]), base);
  }

  #[test]
  fn no_recents_folder_is_a_noop() {
    let base = BrowseResult {
      entries: vec![BrowseEntry::Folder(BrowseFolder {
        node_id: "playlists".into(),
        title: "Playlists".into(),
        subtitle: None,
        artwork_id: None,
        total: None,
        preview_children: Some(vec![track_entry("p")]),
      })],
      total: None,
      has_more: false,
    };
    assert_eq!(overlay_home_recents(base.clone(), &[qi("x")]), base);
  }

  #[test]
  fn live_listens_front_the_shelf_deduped_and_capped() {
    let base = recents_base(vec![track_entry("a"), track_entry("b"), track_entry("c")]);
    // daemon just listened to z then a; a is already on the cached shelf, so it dedups to one entry.
    let out = overlay_home_recents(base, &[qi("z"), qi("a")]);
    // live listens lead (most-recent first), cached shelf backfills, capped to the original size.
    assert_eq!(shelf_uris(&out), vec!["z", "a", "b"]);
  }
}
