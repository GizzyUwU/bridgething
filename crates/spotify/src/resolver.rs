use std::collections::{HashMap, HashSet};

use crate::{
  client::{FlatItem, Release, SpotifyClient},
  error::Error,
  model::BrowseItem,
};

pub type VoiceResult<T> = std::result::Result<T, VoiceResolveError>;

const SEARCH_LIMIT: u32 = 20;
const ALTERNATIVES: usize = 4;
const NEW_RELEASES_TAG: &str = "tag:new";
const CHART_QUERY: &str = "top hits";
const DISCOGRAPHY_DEPTH: usize = 8;
const RECENT_POOL: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceTargetKind {
  Track,
  Album,
  Artist,
  Playlist,
  Show,
  Episode,
  Station,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoicePopularity {
  Top5,
  Top10,
  Popular,
  Recent,
  New,
  Random,
}

#[derive(Debug, Clone, Default)]
pub struct VoiceResolveRequest {
  pub target: Option<String>,
  pub target_type: Option<VoiceTargetKind>,
  pub mood: Option<String>,
  pub genre: Option<String>,
  pub era: Option<String>,
  pub popularity_filter: Option<VoicePopularity>,
  pub position: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceAlternative {
  pub uri: String,
  pub display: String,
  pub kind: VoiceTargetKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceResolved {
  pub uri: String,
  pub context_uri: Option<String>,
  pub display: String,
  pub kind: VoiceTargetKind,
  pub alternatives: Vec<VoiceAlternative>,
}

#[derive(Debug, thiserror::Error)]
pub enum VoiceResolveError {
  #[error("nothing matched the request")]
  NoMatch,
  #[error("a position needs a context and nothing is playing")]
  NoAnchorContext,
  #[error("{0}")]
  Spotify(#[from] Error),
}

pub(crate) async fn resolve(client: &SpotifyClient, req: VoiceResolveRequest) -> VoiceResult<VoiceResolved> {
  let query = compose_query(&req);
  if req.target_type == Some(VoiceTargetKind::Station) && !query.is_empty() {
    return resolve_station(client, &query, req.target.as_deref()).await;
  }
  if let Some(position) = req.position {
    return resolve_position(client, &req, &query, position).await;
  }
  if let Some(kind) = anchored_kind(&req, &query) {
    return resolve_anchor(client, kind).await;
  }
  match req.popularity_filter {
    Some(VoicePopularity::Random) => resolve_random(client, &req, &query).await,
    Some(VoicePopularity::Recent) => resolve_recent(client, &req, &query).await,
    Some(VoicePopularity::New) => resolve_new(client, &req, &query).await,
    Some(filter) => resolve_popular(client, &req, &query, filter.depth()).await,
    None => resolve_search(client, &req, &query).await,
  }
}

async fn resolve_search(client: &SpotifyClient, req: &VoiceResolveRequest, query: &str) -> VoiceResult<VoiceResolved> {
  if query.is_empty() {
    return Err(VoiceResolveError::NoMatch);
  }
  let items = client.search_flat(query, SEARCH_LIMIT).await?;
  head_of(ranked_search(&items, req), None)
}

async fn resolve_random(client: &SpotifyClient, req: &VoiceResolveRequest, query: &str) -> VoiceResult<VoiceResolved> {
  if query.is_empty() {
    return fresh_pick(client).await;
  }
  let items = client.search_flat(query, SEARCH_LIMIT).await?;
  let mut ranked = ranked_search(&items, req);
  if ranked.is_empty() {
    return fresh_pick(client).await;
  }
  let chosen = rand::random_range(0..ranked.len());
  ranked.rotate_left(chosen);
  head_of(ranked, None)
}

async fn resolve_recent(client: &SpotifyClient, req: &VoiceResolveRequest, query: &str) -> VoiceResult<VoiceResolved> {
  let pool = recent_pool(client).await;
  if !pool.is_empty() {
    let named: Vec<Candidate> = candidates_of(client.hydrate_uris(&pool).await);
    let matched = narrow(named, typed_pick(req), query);
    if let Some(head) = matched.first().cloned() {
      return Ok(resolved(head, &matched[1..], None));
    }
  }
  if query.is_empty() {
    return fresh_pick(client).await;
  }
  resolve_search(client, req, query).await
}

async fn resolve_new(client: &SpotifyClient, req: &VoiceResolveRequest, query: &str) -> VoiceResult<VoiceResolved> {
  if !query.is_empty() && req.target.is_some() {
    let items = client.search_flat(query, SEARCH_LIMIT).await?;
    if let Some(artist) = artist_anchor(&items, req) {
      let albums_only = req.target_type == Some(VoiceTargetKind::Album);
      let releases = client
        .artist_releases(&artist.uri, albums_only, DISCOGRAPHY_DEPTH)
        .await?;
      let latest = latest_first(releases);
      if let Some(head) = latest.first().cloned() {
        return Ok(resolved(head, &latest[1..], Some(artist.uri)));
      }
    }
  }
  let items = client
    .search_flat(&tagged(query, NEW_RELEASES_TAG), SEARCH_LIMIT)
    .await?;
  let ranked = rank(&items, typed_pick(req));
  if let Some(head) = ranked.first().cloned() {
    return Ok(resolved(head, &ranked[1..], None));
  }
  if query.is_empty() {
    return fresh_pick(client).await;
  }
  resolve_search(client, req, query).await
}

async fn resolve_popular(
  client: &SpotifyClient,
  req: &VoiceResolveRequest,
  query: &str,
  depth: Option<usize>,
) -> VoiceResult<VoiceResolved> {
  if query.is_empty() {
    return chart_pick(client, depth).await;
  }
  let items = client.search_flat(query, SEARCH_LIMIT).await?;
  if let Some(artist) = artist_anchor(&items, req) {
    let page = client
      .browse_container(&artist.uri, depth.unwrap_or(ALTERNATIVES + 1) as u32, 0)
      .await?;
    let mut top = candidates_of(page.items);
    truncate(&mut top, depth);
    if let Some(head) = top.first().cloned() {
      return Ok(resolved(head, &top[1..], Some(artist.uri)));
    }
  }
  let ranked = by_popularity(client, ranked_search(&items, req), depth).await;
  if let Some(head) = ranked.first().cloned() {
    return Ok(resolved(head, &ranked[1..], None));
  }
  chart_pick(client, depth).await
}

async fn chart_pick(client: &SpotifyClient, depth: Option<usize>) -> VoiceResult<VoiceResolved> {
  let items = client.search_flat(CHART_QUERY, SEARCH_LIMIT).await?;
  let mut ranked = rank(&items, Pick::Playlist);
  if ranked.is_empty() {
    ranked = rank(&items, Pick::Any);
  }
  truncate(&mut ranked, depth);
  match ranked.first().cloned() {
    Some(head) => Ok(resolved(head, &ranked[1..], None)),
    None => fresh_pick(client).await,
  }
}

async fn by_popularity(client: &SpotifyClient, ranked: Vec<Candidate>, depth: Option<usize>) -> Vec<Candidate> {
  if ranked.len() < 2 {
    return ranked;
  }
  let uris: Vec<String> = ranked.iter().map(|c| c.uri.clone()).collect();
  let scores = client.popularity_of(&uris).await;
  rank_by(ranked, &scores, depth)
}

fn rank_by(ranked: Vec<Candidate>, scores: &HashMap<String, i32>, depth: Option<usize>) -> Vec<Candidate> {
  let mut out = ranked;
  out.sort_by_key(|c| std::cmp::Reverse(scores.get(&c.uri).copied().unwrap_or(0)));
  truncate(&mut out, depth);
  out
}

async fn recent_pool(client: &SpotifyClient) -> Vec<String> {
  let (contexts, tracks) = tokio::join!(client.recent_context_uris(), client.recent_track_uris());
  let mut seen = HashSet::new();
  contexts
    .unwrap_or_default()
    .into_iter()
    .chain(tracks.unwrap_or_default())
    .filter(|u| kind_of_uri(u).is_some())
    .filter(|u| seen.insert(u.clone()))
    .take(RECENT_POOL)
    .collect()
}

async fn resolve_station(client: &SpotifyClient, query: &str, target: Option<&str>) -> VoiceResult<VoiceResolved> {
  if query.is_empty() {
    return Err(VoiceResolveError::NoMatch);
  }
  let items = client.search_flat(query, SEARCH_LIMIT).await?;
  let named = target.map(str::trim).filter(|t| !t.is_empty()).unwrap_or(query);
  let seeds: Vec<Candidate> = station_seeds(&items, named).iter().map(as_station).collect();
  let head = seeds.first().cloned().ok_or(VoiceResolveError::NoMatch)?;
  Ok(resolved(head, &seeds[1..], None))
}

fn station_seeds(items: &[FlatItem], named: &str) -> Vec<Candidate> {
  let mut seeds = rank(items, Pick::Seed);
  let named_artist = seeds
    .iter()
    .position(|c| c.kind == VoiceTargetKind::Artist && c.display.eq_ignore_ascii_case(named));
  if let Some(idx) = named_artist {
    seeds[..=idx].rotate_right(1);
  }
  seeds
}

fn station_uri(seed: &str) -> String {
  seed.replacen("spotify:", "spotify:station:", 1)
}

fn as_station(seed: &Candidate) -> Candidate {
  Candidate {
    uri: station_uri(&seed.uri),
    display: seed.display.clone(),
    kind: VoiceTargetKind::Station,
  }
}

fn anchored_kind(req: &VoiceResolveRequest, query: &str) -> Option<VoiceTargetKind> {
  (query.is_empty() && req.popularity_filter.is_none())
    .then_some(req.target_type)
    .flatten()
}

async fn resolve_anchor(client: &SpotifyClient, kind: VoiceTargetKind) -> VoiceResult<VoiceResolved> {
  let anchor = client
    .playback_anchor()
    .await
    .ok_or(VoiceResolveError::NoAnchorContext)?;
  let found = [
    Some(anchor.track_uri),
    anchor.album_uri,
    anchor.artist_uri.clone(),
    anchor.context_uri.clone(),
  ]
  .into_iter()
  .flatten()
  .find(|uri| kind_of_uri(uri) == Some(kind));
  let uri = match (found, kind) {
    (Some(uri), _) => uri,
    (None, VoiceTargetKind::Station) => station_uri(&anchor.artist_uri.ok_or(VoiceResolveError::NoAnchorContext)?),
    (None, _) => return Err(VoiceResolveError::NoAnchorContext),
  };
  let context = matches!(kind, VoiceTargetKind::Track | VoiceTargetKind::Episode)
    .then_some(anchor.context_uri)
    .flatten();
  head_of(candidates_of(client.hydrate_uris(&[uri]).await), context)
}

async fn resolve_position(
  client: &SpotifyClient,
  req: &VoiceResolveRequest,
  query: &str,
  position: u32,
) -> VoiceResult<VoiceResolved> {
  let context = match target_container(client, req, query).await? {
    Some(uri) => uri,
    None => client
      .current_context_uri()
      .await
      .ok_or(VoiceResolveError::NoAnchorContext)?,
  };
  let offset = offset_of(position);
  let page = client
    .browse_container(&context, 1 + ALTERNATIVES as u32, offset)
    .await?;
  head_of(candidates_of(page.items), Some(context))
}

async fn target_container(
  client: &SpotifyClient,
  req: &VoiceResolveRequest,
  query: &str,
) -> VoiceResult<Option<String>> {
  if req.target.is_none() {
    return Ok(None);
  }
  let pick = match req.target_type {
    Some(kind) if is_container(kind) => Pick::Kind(kind),
    Some(_) => return Ok(None),
    None => Pick::Container,
  };
  let items = client.search_flat(query, SEARCH_LIMIT).await?;
  Ok(rank(&items, pick).into_iter().next().map(|c| c.uri))
}

async fn fresh_pick(client: &SpotifyClient) -> VoiceResult<VoiceResolved> {
  let (playlists, home, recents, current) = tokio::join!(
    client.playlist_uris(),
    client.home_uris(),
    client.recent_context_uris(),
    client.current_context_uri(),
  );
  let mut pool = playlists.unwrap_or_default();
  pool.extend(home);
  let mut excluded: HashSet<String> = recents.unwrap_or_default().into_iter().collect();
  excluded.extend(current);
  let pool = fresh_candidates(&pool, &excluded);
  if pool.is_empty() {
    return Err(VoiceResolveError::NoMatch);
  }
  let chosen = pool[rand::random_range(0..pool.len())].clone();
  let mut order = vec![chosen.clone()];
  order.extend(pool.iter().filter(|u| **u != chosen).take(ALTERNATIVES).cloned());
  head_of(candidates_of(client.hydrate_uris(&order).await), None)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Candidate {
  uri: String,
  display: String,
  kind: VoiceTargetKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pick {
  Kind(VoiceTargetKind),
  Any,
  Container,
  Playlist,
  Seed,
}

impl Pick {
  fn keeps(self, kind: VoiceTargetKind) -> bool {
    match self {
      Pick::Kind(want) => want == kind,
      Pick::Any => true,
      Pick::Container => is_container(kind),
      Pick::Playlist => kind == VoiceTargetKind::Playlist,
      Pick::Seed => is_station_seed(kind),
    }
  }
}

impl VoicePopularity {
  fn depth(self) -> Option<usize> {
    match self {
      VoicePopularity::Top5 => Some(5),
      VoicePopularity::Top10 => Some(10),
      _ => None,
    }
  }
}

fn ranked_search(items: &[FlatItem], req: &VoiceResolveRequest) -> Vec<Candidate> {
  let pick = match (req.target.is_some(), req.target_type) {
    (_, Some(kind)) => Pick::Kind(kind),
    (true, None) => Pick::Any,
    (false, None) => Pick::Playlist,
  };
  let ranked = rank(items, pick);
  if ranked.is_empty() && pick == Pick::Playlist {
    return rank(items, Pick::Any);
  }
  ranked
}

fn typed_pick(req: &VoiceResolveRequest) -> Pick {
  match req.target_type {
    Some(VoiceTargetKind::Station) | None => Pick::Any,
    Some(kind) => Pick::Kind(kind),
  }
}

fn head_of(ranked: Vec<Candidate>, context_uri: Option<String>) -> VoiceResult<VoiceResolved> {
  let head = ranked.first().cloned().ok_or(VoiceResolveError::NoMatch)?;
  Ok(resolved(head, &ranked[1..], context_uri))
}

fn candidates_of(items: Vec<BrowseItem>) -> Vec<Candidate> {
  items
    .into_iter()
    .filter_map(|i| {
      kind_of_uri(&i.uri).map(|kind| Candidate {
        uri: i.uri,
        display: i.title,
        kind,
      })
    })
    .collect()
}

fn narrow(candidates: Vec<Candidate>, pick: Pick, query: &str) -> Vec<Candidate> {
  let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
  candidates
    .into_iter()
    .filter(|c| pick.keeps(c.kind))
    .filter(|c| {
      let display = c.display.to_lowercase();
      words.iter().all(|w| display.contains(w.as_str()))
    })
    .collect()
}

fn artist_anchor(items: &[FlatItem], req: &VoiceResolveRequest) -> Option<Candidate> {
  let target = req.target.as_deref().map(str::trim).filter(|t| !t.is_empty())?;
  let artists = rank(items, Pick::Kind(VoiceTargetKind::Artist));
  artists
    .iter()
    .find(|c| c.display.eq_ignore_ascii_case(target))
    .or_else(|| {
      matches!(req.target_type, Some(VoiceTargetKind::Artist | VoiceTargetKind::Album))
        .then(|| artists.first())
        .flatten()
    })
    .cloned()
}

fn latest_first(releases: Vec<Release>) -> Vec<Candidate> {
  let mut releases = releases;
  releases.sort_by_key(|r| (std::cmp::Reverse(r.released), std::cmp::Reverse(r.popularity)));
  releases
    .into_iter()
    .map(|r| Candidate {
      uri: r.uri,
      display: r.name,
      kind: VoiceTargetKind::Album,
    })
    .collect()
}

fn tagged(query: &str, tag: &str) -> String {
  match query.is_empty() {
    true => tag.to_string(),
    false => format!("{query} {tag}"),
  }
}

fn truncate(candidates: &mut Vec<Candidate>, depth: Option<usize>) {
  if let Some(depth) = depth {
    candidates.truncate(depth);
  }
}

fn kind_of_uri(uri: &str) -> Option<VoiceTargetKind> {
  match uri.split(':').nth(1)? {
    "track" => Some(VoiceTargetKind::Track),
    "album" => Some(VoiceTargetKind::Album),
    "artist" => Some(VoiceTargetKind::Artist),
    "playlist" => Some(VoiceTargetKind::Playlist),
    "show" => Some(VoiceTargetKind::Show),
    "episode" => Some(VoiceTargetKind::Episode),
    "station" => Some(VoiceTargetKind::Station),
    _ => None,
  }
}

fn is_station_seed(kind: VoiceTargetKind) -> bool {
  matches!(
    kind,
    VoiceTargetKind::Artist | VoiceTargetKind::Track | VoiceTargetKind::Album | VoiceTargetKind::Playlist
  )
}

fn is_container(kind: VoiceTargetKind) -> bool {
  matches!(
    kind,
    VoiceTargetKind::Album | VoiceTargetKind::Artist | VoiceTargetKind::Playlist | VoiceTargetKind::Show
  )
}

fn is_fresh_context(kind: VoiceTargetKind) -> bool {
  matches!(
    kind,
    VoiceTargetKind::Album | VoiceTargetKind::Artist | VoiceTargetKind::Playlist
  )
}

fn compose_query(req: &VoiceResolveRequest) -> String {
  [
    req.era.as_deref(),
    req.mood.as_deref(),
    req.genre.as_deref(),
    req.target.as_deref(),
  ]
  .into_iter()
  .flatten()
  .map(str::trim)
  .filter(|s| !s.is_empty())
  .collect::<Vec<_>>()
  .join(" ")
}

fn rank(items: &[FlatItem], pick: Pick) -> Vec<Candidate> {
  items
    .iter()
    .filter_map(|i| {
      let kind = kind_of_uri(&i.uri)?;
      pick.keeps(kind).then(|| Candidate {
        uri: i.uri.clone(),
        display: i.name.clone(),
        kind,
      })
    })
    .collect()
}

fn fresh_candidates(pool: &[String], excluded: &HashSet<String>) -> Vec<String> {
  let mut seen = HashSet::new();
  pool
    .iter()
    .filter(|u| kind_of_uri(u).is_some_and(is_fresh_context))
    .filter(|u| !excluded.contains(*u))
    .filter(|u| seen.insert((*u).clone()))
    .cloned()
    .collect()
}

fn offset_of(position: u32) -> u32 {
  position.saturating_sub(1)
}

fn resolved(head: Candidate, rest: &[Candidate], context_uri: Option<String>) -> VoiceResolved {
  VoiceResolved {
    uri: head.uri,
    context_uri,
    display: head.display,
    kind: head.kind,
    alternatives: rest
      .iter()
      .take(ALTERNATIVES)
      .map(|c| VoiceAlternative {
        uri: c.uri.clone(),
        display: c.display.clone(),
        kind: c.kind,
      })
      .collect(),
  }
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use super::*;
  use crate::client::{
    flatten_search,
    tests::{NullObserver, Playing, playing_client, search_response, searching_client, test_client},
  };

  fn flat(loose: &[&str]) -> Vec<FlatItem> {
    flatten_search(&search_response(loose, &[]))
  }

  fn req(target: Option<&str>, target_type: Option<VoiceTargetKind>) -> VoiceResolveRequest {
    VoiceResolveRequest {
      target: target.map(str::to_string),
      target_type,
      ..Default::default()
    }
  }

  fn filtered(filter: VoicePopularity) -> VoiceResolveRequest {
    VoiceResolveRequest {
      popularity_filter: Some(filter),
      ..Default::default()
    }
  }

  fn cand(uri: &str, display: &str) -> Candidate {
    Candidate {
      uri: uri.to_string(),
      display: display.to_string(),
      kind: kind_of_uri(uri).expect("a test candidate is playable"),
    }
  }

  fn release(uri: &str, released: (i32, i32, i32), popularity: i32) -> Release {
    Release {
      uri: uri.to_string(),
      name: uri.rsplit(':').next().unwrap().to_uppercase(),
      released,
      popularity,
    }
  }

  fn picked(out: &VoiceResolved) -> Vec<&str> {
    std::iter::once(out.uri.as_str())
      .chain(out.alternatives.iter().map(|a| a.uri.as_str()))
      .collect()
  }

  #[test]
  fn a_requested_kind_picks_the_first_item_of_that_kind() {
    let items = flat(&[
      "spotify:track:t1",
      "spotify:album:a1",
      "spotify:album:a2",
      "spotify:artist:r1",
    ]);
    let ranked = rank(&items, Pick::Kind(VoiceTargetKind::Album));
    assert_eq!(
      ranked.iter().map(|c| c.uri.as_str()).collect::<Vec<_>>(),
      ["spotify:album:a1", "spotify:album:a2"],
      "a typed pick sees only that kind, still in relevance order"
    );
    assert_eq!(ranked[0].display, "A1");
    assert_eq!(ranked[0].kind, VoiceTargetKind::Album);
  }

  #[test]
  fn an_untyped_pick_keeps_the_global_relevance_order_across_kinds() {
    let items = flat(&["spotify:artist:r1", "spotify:track:t1", "spotify:album:a1"]);
    let ranked = rank(&items, Pick::Any);
    assert_eq!(
      ranked.iter().map(|c| c.uri.as_str()).collect::<Vec<_>>(),
      ["spotify:artist:r1", "spotify:track:t1", "spotify:album:a1"],
      "bucketing would float the track to the front; the flat order must survive"
    );
  }

  #[test]
  fn unplayable_uris_never_become_candidates() {
    let items = flat(&["spotify:user:nobody", "spotify:genre:rock", "spotify:track:t1"]);
    let ranked = rank(&items, Pick::Any);
    assert_eq!(
      ranked.iter().map(|c| c.uri.as_str()).collect::<Vec<_>>(),
      ["spotify:track:t1"]
    );
  }

  #[test]
  fn containers_exclude_leaves() {
    let items = flat(&[
      "spotify:track:t1",
      "spotify:episode:e1",
      "spotify:playlist:p1",
      "spotify:show:s1",
    ]);
    let ranked = rank(&items, Pick::Container);
    assert_eq!(
      ranked.iter().map(|c| c.uri.as_str()).collect::<Vec<_>>(),
      ["spotify:playlist:p1", "spotify:show:s1"],
      "a position counts into a container, never into a track"
    );
  }

  #[test]
  fn alternatives_are_the_next_ranked_candidates_and_are_capped() {
    let items = flat(&[
      "spotify:track:t1",
      "spotify:track:t2",
      "spotify:track:t3",
      "spotify:track:t4",
      "spotify:track:t5",
      "spotify:track:t6",
    ]);
    let ranked = rank(&items, Pick::Any);
    let out = resolved(ranked[0].clone(), &ranked[1..], None);
    assert_eq!(out.uri, "spotify:track:t1");
    assert_eq!(out.context_uri, None, "a track carries no context of its own");
    assert_eq!(
      out.alternatives.iter().map(|a| a.uri.as_str()).collect::<Vec<_>>(),
      [
        "spotify:track:t2",
        "spotify:track:t3",
        "spotify:track:t4",
        "spotify:track:t5"
      ]
    );
  }

  #[test]
  fn a_container_pick_is_its_own_context() {
    let items = flat(&["spotify:album:a1"]);
    let ranked = rank(&items, Pick::Kind(VoiceTargetKind::Album));
    let out = resolved(ranked[0].clone(), &[], None);
    assert_eq!(out.uri, "spotify:album:a1");
    assert_eq!(
      out.context_uri, None,
      "the album uri is the context; it is not repeated"
    );
  }

  #[test]
  fn positions_are_one_based_offsets() {
    assert_eq!(offset_of(1), 0);
    assert_eq!(offset_of(3), 2);
    assert_eq!(offset_of(0), 0, "a zeroth item is the first item, not an underflow");
  }

  #[test]
  fn query_composition_reads_era_mood_genre_then_target() {
    let composed = |era, mood, genre, target: Option<&str>| {
      compose_query(&VoiceResolveRequest {
        era: Option::<&str>::map(era, str::to_string),
        mood: Option::<&str>::map(mood, str::to_string),
        genre: Option::<&str>::map(genre, str::to_string),
        target: target.map(str::to_string),
        ..Default::default()
      })
    };
    assert_eq!(composed(Some("80s"), None, Some("rock"), None), "80s rock");
    assert_eq!(composed(None, Some("chill"), Some("jazz"), None), "chill jazz");
    assert_eq!(composed(Some("90s"), None, None, Some("radiohead")), "90s radiohead");
    assert_eq!(composed(None, None, None, Some(" daft punk ")), "daft punk");
    assert_eq!(composed(None, None, None, None), "");
  }

  #[test]
  fn a_fresh_pick_drops_recents_and_whatever_is_playing() {
    let pool: Vec<String> = [
      "spotify:playlist:p1",
      "spotify:playlist:p2",
      "spotify:album:a1",
      "spotify:playlist:p3",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let excluded: HashSet<String> = ["spotify:playlist:p2", "spotify:playlist:p3"]
      .iter()
      .map(|s| s.to_string())
      .collect();
    assert_eq!(
      fresh_candidates(&pool, &excluded),
      ["spotify:playlist:p1", "spotify:album:a1"],
      "a fresh pick can never resume the current context or replay a recent one"
    );
  }

  #[test]
  fn a_fresh_pick_only_considers_playable_music_contexts() {
    let pool: Vec<String> = [
      "spotify:track:t1",
      "spotify:episode:e1",
      "spotify:show:s1",
      "spotify:user:me:collection",
      "spotify:artist:r1",
      "spotify:playlist:p1",
      "spotify:playlist:p1",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(
      fresh_candidates(&pool, &HashSet::new()),
      ["spotify:artist:r1", "spotify:playlist:p1"],
      "leaves, shows and duplicates are not fresh contexts"
    );
  }

  #[test]
  fn a_station_wraps_its_seed_uri_whatever_the_seed_kind() {
    for (seed, want) in [
      ("spotify:artist:r1", "spotify:station:artist:r1"),
      ("spotify:track:t1", "spotify:station:track:t1"),
      ("spotify:album:a1", "spotify:station:album:a1"),
      ("spotify:playlist:p1", "spotify:station:playlist:p1"),
    ] {
      let items = flat(&[seed]);
      let seeds = station_seeds(&items, "whatever");
      let out = as_station(&seeds[0]);
      assert_eq!(out.uri, want);
      assert_eq!(out.kind, VoiceTargetKind::Station);
      assert_eq!(out.display, seeds[0].display, "the seed name is the display");
      assert_eq!(
        kind_of_uri(&out.uri),
        Some(VoiceTargetKind::Station),
        "a synthesized station uri reads back as one"
      );
    }
  }

  #[test]
  fn a_station_never_seeds_off_a_podcast() {
    let items = flat(&["spotify:show:s1", "spotify:episode:e1", "spotify:album:a1"]);
    assert_eq!(
      station_seeds(&items, "q")
        .iter()
        .map(|c| c.uri.as_str())
        .collect::<Vec<_>>(),
      ["spotify:album:a1"],
      "the server rejects podcast seeds outright"
    );
  }

  #[test]
  fn a_station_prefers_the_artist_over_a_same_named_playlist() {
    let items = flat(&["spotify:playlist:p1", "spotify:track:t1", "spotify:artist:r1"]);
    let seeds = station_seeds(&items, "R1");
    assert_eq!(
      seeds.iter().map(|c| c.uri.as_str()).collect::<Vec<_>>(),
      ["spotify:artist:r1", "spotify:playlist:p1", "spotify:track:t1"],
      "search floats the editorial radio playlist first; the artist itself is the better seed"
    );
  }

  #[test]
  fn a_station_matches_the_artist_name_against_the_target_not_the_modifiers() {
    let items = flat(&["spotify:playlist:p1", "spotify:artist:r1"]);
    let composed = compose_query(&VoiceResolveRequest {
      era: Some("80s".into()),
      target: Some("r1".into()),
      target_type: Some(VoiceTargetKind::Station),
      ..Default::default()
    });
    assert_eq!(composed, "80s r1");
    assert_eq!(
      station_seeds(&items, "r1").first().map(|c| c.uri.as_str()),
      Some("spotify:artist:r1"),
      "an era modifier must not stop the artist name from matching"
    );
    assert_eq!(
      station_seeds(&items, &composed).first().map(|c| c.uri.as_str()),
      Some("spotify:playlist:p1"),
      "matching on the composed query is what the target-slot match avoids"
    );
  }

  #[test]
  fn a_station_falls_back_to_relevance_when_no_artist_carries_the_query_name() {
    let items = flat(&["spotify:track:t1", "spotify:artist:r1"]);
    let seeds = station_seeds(&items, "bohemian rhapsody");
    assert_eq!(
      seeds.first().map(|c| c.uri.as_str()),
      Some("spotify:track:t1"),
      "naming a song must still seed a track station"
    );
  }

  #[tokio::test]
  async fn a_station_with_nothing_to_seed_from_matches_nothing() {
    let client = test_client(Arc::new(NullObserver));
    let err = resolve(&client, req(None, Some(VoiceTargetKind::Station)))
      .await
      .unwrap_err();
    assert!(
      matches!(err, VoiceResolveError::NoAnchorContext),
      "a station is never synthesized from thin air; with nothing on there is nothing to seed it: {err:?}"
    );
  }

  #[tokio::test]
  async fn an_empty_request_matches_nothing_rather_than_resuming() {
    let client = test_client(Arc::new(NullObserver));
    let err = resolve(&client, VoiceResolveRequest::default()).await.unwrap_err();
    assert!(
      matches!(err, VoiceResolveError::NoMatch),
      "no slots and no random filter is not a resume: {err:?}"
    );
  }

  // ---- popularity filters --------------------------------------------------

  #[test]
  fn only_the_counted_filters_bound_how_deep_a_ranking_is_read() {
    assert_eq!(VoicePopularity::Top5.depth(), Some(5));
    assert_eq!(VoicePopularity::Top10.depth(), Some(10));
    assert_eq!(
      VoicePopularity::Popular.depth(),
      None,
      "an uncounted filter reads the whole ranking"
    );
  }

  #[test]
  fn a_depth_bounds_the_pool_the_alternatives_come_from() {
    let mut pool: Vec<Candidate> = (1..=8).map(|n| cand(&format!("spotify:track:t{n}"), "T")).collect();
    truncate(&mut pool, Some(5));
    assert_eq!(pool.len(), 5);
    let mut whole: Vec<Candidate> = (1..=8).map(|n| cand(&format!("spotify:track:t{n}"), "T")).collect();
    truncate(&mut whole, None);
    assert_eq!(whole.len(), 8, "no depth keeps every candidate");
  }

  #[test]
  fn the_new_release_tag_rides_the_composed_query_and_stands_alone_without_one() {
    assert_eq!(tagged("80s rock", NEW_RELEASES_TAG), "80s rock tag:new");
    assert_eq!(
      tagged("", NEW_RELEASES_TAG),
      "tag:new",
      "with nothing named the tag is the whole query"
    );
  }

  #[test]
  fn a_filtered_pick_narrows_by_kind_but_never_by_station() {
    assert_eq!(typed_pick(&req(None, None)), Pick::Any);
    assert_eq!(
      typed_pick(&req(None, Some(VoiceTargetKind::Album))),
      Pick::Kind(VoiceTargetKind::Album)
    );
    assert_eq!(
      typed_pick(&req(None, Some(VoiceTargetKind::Station))),
      Pick::Any,
      "search never returns a station uri, so narrowing to one would empty every ranking"
    );
  }

  #[test]
  fn history_keeps_only_entries_carrying_every_word_that_was_spoken() {
    let history = vec![
      cand("spotify:playlist:p1", "Deep Focus"),
      cand("spotify:album:a1", "Deep Cuts"),
      cand("spotify:track:t1", "Focus Deep Down"),
    ];
    assert_eq!(
      narrow(history.clone(), Pick::Any, "deep focus")
        .iter()
        .map(|c| c.uri.as_str())
        .collect::<Vec<_>>(),
      ["spotify:playlist:p1", "spotify:track:t1"],
      "word order is not position; both entries carry both words, in relevance order"
    );
    assert_eq!(
      narrow(history.clone(), Pick::Kind(VoiceTargetKind::Track), "deep")
        .iter()
        .map(|c| c.uri.as_str())
        .collect::<Vec<_>>(),
      ["spotify:track:t1"],
      "a named kind still narrows the history"
    );
    assert_eq!(
      narrow(history, Pick::Any, "").len(),
      3,
      "nothing spoken keeps the whole history"
    );
  }

  #[test]
  fn the_named_artist_outranks_relevance_as_the_anchor_of_a_filter() {
    let items = flat(&["spotify:artist:r1", "spotify:artist:r2", "spotify:track:t1"]);
    assert_eq!(
      artist_anchor(&items, &req(Some("r2"), None)).map(|c| c.uri),
      Some("spotify:artist:r2".to_string()),
      "the artist the user named is the anchor even when search floats another first"
    );
    assert_eq!(
      artist_anchor(&items, &req(Some("bohemian rhapsody"), None)),
      None,
      "an untyped name that matches no artist is not silently read as one"
    );
    assert_eq!(
      artist_anchor(&items, &req(Some("bohemian rhapsody"), Some(VoiceTargetKind::Album))).map(|c| c.uri),
      Some("spotify:artist:r1".to_string()),
      "asking for an album by a name takes the best artist even without an exact match"
    );
    assert_eq!(
      artist_anchor(&items, &req(None, Some(VoiceTargetKind::Artist))),
      None,
      "no name means no anchor, whatever the kind"
    );
  }

  #[test]
  fn the_latest_release_is_the_newest_date_and_the_canonical_cut_of_it() {
    let latest = latest_first(vec![
      release("spotify:album:deluxe", (2026, 5, 15), 84),
      release("spotify:album:flagship", (2026, 5, 15), 96),
      release("spotify:album:older", (2025, 2, 14), 99),
    ]);
    assert_eq!(
      latest.iter().map(|c| c.uri.as_str()).collect::<Vec<_>>(),
      ["spotify:album:flagship", "spotify:album:deluxe", "spotify:album:older"],
      "a same-day sibling is a cut of one release; popularity picks the canonical one"
    );
    assert_eq!(latest[0].kind, VoiceTargetKind::Album);
  }

  #[test]
  fn popularity_reorders_the_hits_and_sinks_the_kinds_that_carry_no_score() {
    let ranked = vec![
      cand("spotify:track:t1", "T1"),
      cand("spotify:playlist:p1", "P1"),
      cand("spotify:track:t2", "T2"),
      cand("spotify:playlist:p2", "P2"),
    ];
    let scores = HashMap::from([
      ("spotify:track:t1".to_string(), 41),
      ("spotify:track:t2".to_string(), 84),
    ]);
    assert_eq!(
      rank_by(ranked, &scores, None)
        .iter()
        .map(|c| c.uri.as_str())
        .collect::<Vec<_>>(),
      [
        "spotify:track:t2",
        "spotify:track:t1",
        "spotify:playlist:p1",
        "spotify:playlist:p2"
      ],
      "playlists have no popularity field; they sink together and keep relevance order"
    );
  }

  #[tokio::test]
  async fn a_filter_with_nothing_named_resolves_instead_of_failing() {
    for filter in [
      VoicePopularity::Popular,
      VoicePopularity::Top5,
      VoicePopularity::Top10,
      VoicePopularity::New,
    ] {
      let (client, _) = searching_client(Arc::new(NullObserver), &["spotify:playlist:p1", "spotify:album:a1"]);
      let out = resolve(&client, filtered(filter))
        .await
        .unwrap_or_else(|e| panic!("{filter:?} with an empty query must resolve, got {e:?}"));
      assert!(kind_of_uri(&out.uri).is_some(), "{filter:?} landed on {}", out.uri);
    }
  }

  #[tokio::test]
  async fn nothing_named_asks_the_chart_for_hits_and_the_tag_for_new_releases() {
    let (client, log) = searching_client(Arc::new(NullObserver), &["spotify:playlist:p1"]);
    resolve(&client, filtered(VoicePopularity::Popular)).await.unwrap();
    assert_eq!(log.queries(), [CHART_QUERY], "a bare hits request is the live chart");

    let (client, log) = searching_client(Arc::new(NullObserver), &["spotify:album:a1"]);
    resolve(&client, filtered(VoicePopularity::New)).await.unwrap();
    assert_eq!(log.queries(), [NEW_RELEASES_TAG]);
  }

  #[tokio::test]
  async fn a_new_release_request_without_an_artist_tags_the_composed_query() {
    let (client, log) = searching_client(Arc::new(NullObserver), &["spotify:album:a1"]);
    let out = resolve(
      &client,
      VoiceResolveRequest {
        era: Some("80s".into()),
        genre: Some("rock".into()),
        popularity_filter: Some(VoicePopularity::New),
        ..Default::default()
      },
    )
    .await
    .unwrap();
    assert_eq!(
      log.queries(),
      ["80s rock tag:new"],
      "modifiers with no name never take the discography path"
    );
    assert_eq!(out.uri, "spotify:album:a1");
  }

  #[tokio::test]
  async fn a_history_request_that_the_history_cannot_answer_falls_back_to_the_catalog() {
    let (client, log) = searching_client(Arc::new(NullObserver), &["spotify:playlist:p1"]);
    let out = resolve(
      &client,
      VoiceResolveRequest {
        genre: Some("jazz".into()),
        popularity_filter: Some(VoicePopularity::Recent),
        ..Default::default()
      },
    )
    .await
    .expect("an unreachable history degrades rather than failing");
    assert_eq!(log.queries(), ["jazz"], "the fallback is the plain unfiltered search");
    assert_eq!(picked(&out), ["spotify:playlist:p1"]);
  }

  #[tokio::test]
  async fn a_station_with_no_seed_still_answers_when_a_filter_carries_the_request() {
    let (client, log) = searching_client(Arc::new(NullObserver), &["spotify:album:a1"]);
    let out = resolve(
      &client,
      VoiceResolveRequest {
        target_type: Some(VoiceTargetKind::Station),
        popularity_filter: Some(VoicePopularity::New),
        ..Default::default()
      },
    )
    .await
    .expect("a seedless station degrades to the filter rather than failing");
    assert_eq!(log.queries(), [NEW_RELEASES_TAG]);
    assert_eq!(out.uri, "spotify:album:a1");
  }

  #[tokio::test]
  async fn a_named_station_still_outranks_a_filter() {
    let (client, log) = searching_client(Arc::new(NullObserver), &["spotify:artist:r1"]);
    let out = resolve(
      &client,
      VoiceResolveRequest {
        target: Some("r1".into()),
        target_type: Some(VoiceTargetKind::Station),
        popularity_filter: Some(VoicePopularity::Popular),
        ..Default::default()
      },
    )
    .await
    .expect("station resolve");
    assert_eq!(log.queries(), ["r1"], "a seeded station never reaches the filter paths");
    assert_eq!(out.uri, "spotify:station:artist:r1");
  }

  #[tokio::test]
  async fn a_position_without_a_target_or_playback_is_a_typed_error() {
    let client = test_client(Arc::new(NullObserver));
    let err = resolve(
      &client,
      VoiceResolveRequest {
        position: Some(3),
        ..Default::default()
      },
    )
    .await
    .unwrap_err();
    assert!(
      matches!(err, VoiceResolveError::NoAnchorContext),
      "nothing playing means nothing to count into: {err:?}"
    );
  }

  // ---- bare kinds anchored to now playing ----------------------------------

  fn bare(kind: VoiceTargetKind) -> VoiceResolveRequest {
    req(None, Some(kind))
  }

  async fn anchored_client(playing: Playing<'_>) -> SpotifyClient {
    playing_client(Arc::new(NullObserver), playing).await
  }

  #[tokio::test]
  async fn a_bare_album_kind_plays_the_album_of_the_current_track() {
    let client = anchored_client(Playing {
      track: "spotify:track:t1",
      album: "spotify:album:a1",
      artist: "spotify:artist:r1",
      context: "spotify:playlist:p1",
    })
    .await;
    let out = resolve(&client, bare(VoiceTargetKind::Album)).await.expect("anchor");
    assert_eq!(out.uri, "spotify:album:a1");
    assert_eq!(out.kind, VoiceTargetKind::Album);
    assert_eq!(out.context_uri, None, "an album is its own context");
    assert!(out.alternatives.is_empty(), "the anchor is the only answer");
  }

  #[tokio::test]
  async fn a_bare_artist_kind_plays_the_artist_of_the_current_track() {
    let client = anchored_client(Playing {
      track: "spotify:track:t1",
      album: "spotify:album:a1",
      artist: "spotify:artist:r1",
      ..Default::default()
    })
    .await;
    let out = resolve(&client, bare(VoiceTargetKind::Artist)).await.expect("anchor");
    assert_eq!(out.uri, "spotify:artist:r1");
    assert_eq!(out.context_uri, None);
  }

  #[tokio::test]
  async fn a_bare_track_kind_replays_the_current_track_inside_its_context() {
    let client = anchored_client(Playing {
      track: "spotify:track:t1",
      album: "spotify:album:a1",
      context: "spotify:playlist:p1",
      ..Default::default()
    })
    .await;
    let out = resolve(&client, bare(VoiceTargetKind::Track)).await.expect("anchor");
    assert_eq!(out.uri, "spotify:track:t1");
    assert_eq!(
      out.context_uri.as_deref(),
      Some("spotify:playlist:p1"),
      "a leaf keeps the context it is playing inside"
    );
  }

  #[tokio::test]
  async fn a_bare_playlist_kind_takes_the_context_that_is_playing() {
    let client = anchored_client(Playing {
      track: "spotify:track:t1",
      album: "spotify:album:a1",
      context: "spotify:playlist:p1",
      ..Default::default()
    })
    .await;
    let out = resolve(&client, bare(VoiceTargetKind::Playlist)).await.expect("anchor");
    assert_eq!(out.uri, "spotify:playlist:p1");
    assert_eq!(out.context_uri, None);
  }

  #[tokio::test]
  async fn a_bare_playlist_kind_over_an_album_context_is_a_typed_error() {
    let client = anchored_client(Playing {
      track: "spotify:track:t1",
      album: "spotify:album:a1",
      context: "spotify:album:a1",
      ..Default::default()
    })
    .await;
    let err = resolve(&client, bare(VoiceTargetKind::Playlist)).await.unwrap_err();
    assert!(
      matches!(err, VoiceResolveError::NoAnchorContext),
      "no playlist is playing, so there is nothing to read the kind against: {err:?}"
    );
  }

  #[tokio::test]
  async fn a_bare_episode_kind_plays_the_episode_that_is_on() {
    let client = anchored_client(Playing {
      track: "spotify:episode:e1",
      context: "spotify:show:s1",
      ..Default::default()
    })
    .await;
    let out = resolve(&client, bare(VoiceTargetKind::Episode)).await.expect("anchor");
    assert_eq!(
      out.uri, "spotify:episode:e1",
      "the playing episode is the leaf, not the show"
    );
    assert_eq!(out.context_uri.as_deref(), Some("spotify:show:s1"));

    let show = resolve(&client, bare(VoiceTargetKind::Show)).await.expect("anchor");
    assert_eq!(show.uri, "spotify:show:s1");
    assert_eq!(show.context_uri, None);
  }

  #[tokio::test]
  async fn a_bare_station_kind_needs_a_station_context() {
    let client = anchored_client(Playing {
      track: "spotify:track:t1",
      context: "spotify:station:artist:r1",
      ..Default::default()
    })
    .await;
    let out = resolve(&client, bare(VoiceTargetKind::Station)).await.expect("anchor");
    assert_eq!(out.uri, "spotify:station:artist:r1");
  }

  #[tokio::test]
  async fn a_bare_station_kind_over_a_plain_context_seeds_radio_from_the_playing_artist() {
    let client = anchored_client(Playing {
      track: "spotify:track:t1",
      artist: "spotify:artist:a1",
      context: "spotify:playlist:p1",
      ..Default::default()
    })
    .await;
    let out = resolve(&client, bare(VoiceTargetKind::Station)).await.expect("seeded");
    assert_eq!(out.uri, "spotify:station:artist:a1");
  }

  #[tokio::test]
  async fn a_bare_station_kind_with_no_artist_on_the_track_is_a_typed_error() {
    let client = anchored_client(Playing {
      track: "spotify:track:t1",
      context: "spotify:playlist:p1",
      ..Default::default()
    })
    .await;
    let out = resolve(&client, bare(VoiceTargetKind::Station)).await;
    assert!(matches!(out, Err(VoiceResolveError::NoAnchorContext)));
  }

  #[tokio::test]
  async fn a_bare_kind_with_nothing_playing_is_a_typed_error() {
    let client = test_client(Arc::new(NullObserver));
    for kind in [
      VoiceTargetKind::Album,
      VoiceTargetKind::Artist,
      VoiceTargetKind::Track,
      VoiceTargetKind::Playlist,
      VoiceTargetKind::Show,
      VoiceTargetKind::Episode,
      VoiceTargetKind::Station,
    ] {
      let err = resolve(&client, bare(kind)).await.unwrap_err();
      assert!(
        matches!(err, VoiceResolveError::NoAnchorContext),
        "{kind:?} has nothing to anchor against: {err:?}"
      );
    }
  }

  #[tokio::test]
  async fn a_bare_kind_carrying_a_filter_still_belongs_to_the_filter() {
    let (client, log) = searching_client(Arc::new(NullObserver), &["spotify:playlist:p1"]);
    let out = resolve(
      &client,
      VoiceResolveRequest {
        target_type: Some(VoiceTargetKind::Album),
        popularity_filter: Some(VoicePopularity::Popular),
        ..Default::default()
      },
    )
    .await
    .expect("the filter answers");
    assert_eq!(log.queries(), [CHART_QUERY], "a filter asks the world, not what is on");
    assert_eq!(out.uri, "spotify:playlist:p1");
  }
}

#[cfg(all(test, feature = "native-io"))]
mod live {
  use std::{path::PathBuf, sync::Arc};

  use super::*;
  use crate::{
    auth::{Auth, DEFAULT_WORKER_BASE},
    client::Observer,
    http::{SpHttp, random_hex},
    httpx,
    model::{AuthState, Device, LibraryScope, PlayerState, Queue},
    spclient::SpClient,
    store::FileTokenStore,
  };

  struct Silent;
  impl Observer for Silent {
    fn on_player(&self, _state: PlayerState) {}
    fn on_queue(&self, _queue: Queue) {}
    fn on_devices(&self, _devices: Vec<Device>) {}
    fn on_auth(&self, _state: AuthState) {}
    fn on_library_changed(&self, _scope: LibraryScope) {}
  }

  fn enabled() -> bool {
    std::env::var("SPOTIFY_LIVE").as_deref() == Ok("1")
  }

  fn state_dir() -> PathBuf {
    match std::env::var("SPOTIFY_PRIVATE_STATE") {
      Ok(dir) => PathBuf::from(dir),
      Err(_) => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.spotify-private"),
    }
  }

  static AUTH: tokio::sync::OnceCell<Arc<Auth>> = tokio::sync::OnceCell::const_new();

  async fn auth() -> Arc<Auth> {
    AUTH
      .get_or_init(|| async {
        let psk = std::env::var("SPOTIFY_AUTH_PSK").expect("SPOTIFY_AUTH_PSK gates the private-auth worker");
        let base = std::env::var("SPOTIFY_AUTH_BASE").unwrap_or_else(|_| DEFAULT_WORKER_BASE.to_string());
        let dir = state_dir();
        let store = FileTokenStore::new(&dir).expect("private state dir");
        let auth = Arc::new(Auth::new(base, psk, Box::new(store), httpx::executor()));
        assert!(
          auth.is_paired().await,
          "live lane needs a paired refresh token in {}; run `sfp pair` first",
          dir.display()
        );
        auth
      })
      .await
      .clone()
  }

  async fn client() -> SpotifyClient {
    let client = SpotifyClient::new(auth().await, random_hex(20), httpx::executor(), Arc::new(Silent));
    client.connect().await.expect("live connect");
    client
  }

  fn show(label: &str, out: &VoiceResolved) {
    println!(
      "[{label}] {} kind={:?} context={:?} display={:?} alts={}",
      out.uri,
      out.kind,
      out.context_uri,
      out.display,
      out.alternatives.len()
    );
  }

  #[tokio::test]
  async fn live_resolution_lands_on_real_uris_without_ever_commanding_playback() {
    if !enabled() {
      return;
    }
    let client = client().await;

    let artist = resolve(
      &client,
      VoiceResolveRequest {
        target: Some("taylor swift".into()),
        target_type: Some(VoiceTargetKind::Artist),
        ..Default::default()
      },
    )
    .await
    .expect("typed artist resolve");
    show("artist", &artist);
    assert!(artist.uri.starts_with("spotify:artist:"), "got {}", artist.uri);
    assert_eq!(artist.kind, VoiceTargetKind::Artist);
    assert_eq!(artist.context_uri, None, "an artist uri is already the context");

    let untyped = resolve(
      &client,
      VoiceResolveRequest {
        target: Some("bohemian rhapsody".into()),
        ..Default::default()
      },
    )
    .await
    .expect("untyped resolve");
    show("untyped", &untyped);
    assert!(kind_of_uri(&untyped.uri).is_some(), "got {}", untyped.uri);

    let genre = resolve(
      &client,
      VoiceResolveRequest {
        genre: Some("jazz".into()),
        mood: Some("chill".into()),
        ..Default::default()
      },
    )
    .await
    .expect("genre resolve");
    show("genre+mood", &genre);
    assert!(kind_of_uri(&genre.uri).is_some(), "got {}", genre.uri);

    let era = resolve(
      &client,
      VoiceResolveRequest {
        era: Some("80s".into()),
        genre: Some("rock".into()),
        ..Default::default()
      },
    )
    .await
    .expect("era resolve");
    show("era+genre", &era);
    assert!(kind_of_uri(&era.uri).is_some(), "got {}", era.uri);

    let position = resolve(
      &client,
      VoiceResolveRequest {
        target: Some("rumours fleetwood mac".into()),
        target_type: Some(VoiceTargetKind::Album),
        position: Some(3),
        ..Default::default()
      },
    )
    .await
    .expect("position resolve");
    show("position", &position);
    assert!(position.uri.starts_with("spotify:track:"), "got {}", position.uri);
    assert!(
      position
        .context_uri
        .as_deref()
        .is_some_and(|c| c.starts_with("spotify:album:")),
      "a position always reports the container it counted into: {:?}",
      position.context_uri
    );

    let playing = client.current_context_uri().await;
    let recents = client.recent_context_uris().await.unwrap_or_default();
    let random = resolve(
      &client,
      VoiceResolveRequest {
        popularity_filter: Some(VoicePopularity::Random),
        ..Default::default()
      },
    )
    .await
    .expect("fresh pick");
    show("random", &random);
    assert!(
      kind_of_uri(&random.uri).is_some_and(is_fresh_context),
      "a fresh pick is a playable music context: {}",
      random.uri
    );
    assert_ne!(
      Some(random.uri.clone()),
      playing,
      "a fresh pick must never be a resume of what is already on"
    );
    assert!(
      !recents.contains(&random.uri),
      "a fresh pick must never replay a recent context: {}",
      random.uri
    );

    for seed in ["elvis presley", "bohemian rhapsody"] {
      let station = resolve(
        &client,
        VoiceResolveRequest {
          target: Some(seed.into()),
          target_type: Some(VoiceTargetKind::Station),
          ..Default::default()
        },
      )
      .await
      .expect("station resolve");
      show(&format!("station {seed}"), &station);
      assert!(station.uri.starts_with("spotify:station:"), "got {}", station.uri);
      assert_eq!(station.kind, VoiceTargetKind::Station);
      assert_eq!(station.context_uri, None, "a station uri is already the context");
      assert_station_resolves(&station.uri).await;
    }

    client.disconnect().await;
  }

  #[tokio::test]
  async fn live_every_popularity_filter_answers_with_nothing_else_named() {
    if !enabled() {
      return;
    }
    let client = client().await;
    for filter in [
      VoicePopularity::Popular,
      VoicePopularity::Top5,
      VoicePopularity::Top10,
      VoicePopularity::New,
      VoicePopularity::Recent,
      VoicePopularity::Random,
    ] {
      let out = resolve(
        &client,
        VoiceResolveRequest {
          popularity_filter: Some(filter),
          ..Default::default()
        },
      )
      .await
      .unwrap_or_else(|e| panic!("a bare {filter:?} request must never fail the turn: {e:?}"));
      show(&format!("bare {filter:?}"), &out);
      assert!(
        kind_of_uri(&out.uri).is_some(),
        "{filter:?} landed on an unplayable uri: {}",
        out.uri
      );
    }
    client.disconnect().await;
  }

  #[tokio::test]
  async fn live_a_named_artist_reads_their_own_discography_and_top_tracks() {
    if !enabled() {
      return;
    }
    let client = client().await;

    let latest = resolve(
      &client,
      VoiceResolveRequest {
        target: Some("taylor swift".into()),
        target_type: Some(VoiceTargetKind::Album),
        popularity_filter: Some(VoicePopularity::New),
        ..Default::default()
      },
    )
    .await
    .expect("latest album resolve");
    show("latest album", &latest);
    assert!(latest.uri.starts_with("spotify:album:"), "got {}", latest.uri);
    assert_eq!(latest.kind, VoiceTargetKind::Album);
    assert_eq!(
      latest.context_uri.as_deref(),
      Some("spotify:artist:06HL4z0CvFAxyc27GXpf02"),
      "a discography pick reports the artist it counted into"
    );

    let hit = resolve(
      &client,
      VoiceResolveRequest {
        target: Some("taylor swift".into()),
        popularity_filter: Some(VoicePopularity::Top5),
        ..Default::default()
      },
    )
    .await
    .expect("top track resolve");
    show("top track", &hit);
    assert!(hit.uri.starts_with("spotify:track:"), "got {}", hit.uri);
    assert!(
      hit.alternatives.len() <= 4,
      "a counted filter never offers more than the alternatives cap"
    );

    client.disconnect().await;
  }

  #[tokio::test]
  async fn live_the_new_release_tag_still_narrows_the_catalog() {
    if !enabled() {
      return;
    }
    let client = client().await;
    let items = client
      .search_flat(NEW_RELEASES_TAG, SEARCH_LIMIT)
      .await
      .expect("tagged search");
    let uris: Vec<String> = items.iter().map(|i| i.uri.clone()).collect();
    assert!(!uris.is_empty(), "the tag returned nothing at all");
    assert!(
      uris.iter().all(|u| u.starts_with("spotify:album:")),
      "the tag is an album-only narrowing; got {uris:?}"
    );
    let releases = client.popularity_of(&uris).await;
    assert!(
      !releases.is_empty(),
      "tagged hits must hydrate as real albums, not phantom uris"
    );
    client.disconnect().await;
  }

  async fn assert_station_resolves(uri: &str) {
    let spc = SpClient::new(SpHttp::new(auth().await, httpx::executor()));
    let ctx = spc.context_resolve(uri).await.expect("station context resolves");
    let pages = ctx.get("pages").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let tracks = pages
      .first()
      .and_then(|p| p.get("tracks"))
      .and_then(|v| v.as_array())
      .map(Vec::len)
      .unwrap_or(0);
    let shuffle_reasons = ctx
      .get("restrictions")
      .and_then(|r| r.get("disallow_toggling_shuffle_reasons"))
      .and_then(|v| v.as_array())
      .cloned()
      .unwrap_or_default();
    println!("  {uri} -> {tracks} tracks, shuffle disallowed {shuffle_reasons:?}");
    assert!(tracks > 0, "a station resolves to real tracks");
    assert!(
      shuffle_reasons.iter().any(|r| r.as_str() == Some("radio")),
      "expected the radio shuffle restriction on {uri}, got {shuffle_reasons:?}"
    );
  }
}
