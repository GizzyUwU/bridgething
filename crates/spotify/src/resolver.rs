use std::collections::HashSet;

use crate::{
  client::{FlatItem, SpotifyClient},
  error::Error,
};

pub type VoiceResult<T> = std::result::Result<T, VoiceResolveError>;

const SEARCH_LIMIT: u32 = 20;
const ALTERNATIVES: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum VoiceTargetKind {
  Track,
  Album,
  Artist,
  Playlist,
  Show,
  Episode,
  Station,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum VoicePopularity {
  Top5,
  Top10,
  Popular,
  Recent,
  New,
  Random,
}

#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct VoiceResolveRequest {
  pub target: Option<String>,
  pub target_type: Option<VoiceTargetKind>,
  pub mood: Option<String>,
  pub genre: Option<String>,
  pub era: Option<String>,
  pub popularity_filter: Option<VoicePopularity>,
  pub position: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct VoiceAlternative {
  pub uri: String,
  pub display: String,
  pub kind: VoiceTargetKind,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct VoiceResolved {
  pub uri: String,
  pub context_uri: Option<String>,
  pub display: String,
  pub kind: VoiceTargetKind,
  pub alternatives: Vec<VoiceAlternative>,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
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
  if req.target_type == Some(VoiceTargetKind::Station) {
    return resolve_station(client, &query, req.target.as_deref()).await;
  }
  if let Some(position) = req.position {
    return resolve_position(client, &req, &query, position).await;
  }
  if query.is_empty() {
    if req.popularity_filter == Some(VoicePopularity::Random) {
      return fresh_pick(client).await;
    }
    return Err(VoiceResolveError::NoMatch);
  }
  let pick = match (req.target.is_some(), req.target_type) {
    (_, Some(kind)) => Pick::Kind(kind),
    (true, None) => Pick::Any,
    (false, None) => Pick::Playlist,
  };
  let items = client.search_flat(&query, SEARCH_LIMIT).await?;
  let mut ranked = rank(&items, pick);
  if ranked.is_empty() && pick == Pick::Playlist {
    ranked = rank(&items, Pick::Any);
  }
  let head = ranked.first().cloned().ok_or(VoiceResolveError::NoMatch)?;
  Ok(resolved(head, &ranked[1..], None))
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

fn as_station(seed: &Candidate) -> Candidate {
  Candidate {
    uri: seed.uri.replacen("spotify:", "spotify:station:", 1),
    display: seed.display.clone(),
    kind: VoiceTargetKind::Station,
  }
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
  let mut items = page.items.into_iter();
  let head = items.next().ok_or(VoiceResolveError::NoMatch)?;
  let kind = kind_of_uri(&head.uri).ok_or(VoiceResolveError::NoMatch)?;
  let alternatives = items
    .filter_map(|i| {
      kind_of_uri(&i.uri).map(|k| VoiceAlternative {
        uri: i.uri,
        display: i.title,
        kind: k,
      })
    })
    .collect();
  Ok(VoiceResolved {
    uri: head.uri,
    context_uri: Some(context),
    display: head.title,
    kind,
    alternatives,
  })
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
  let items = client.hydrate_uris(&order).await;
  let mut ranked = items.into_iter().filter_map(|i| {
    kind_of_uri(&i.uri).map(|kind| Candidate {
      uri: i.uri,
      display: i.title,
      kind,
    })
  });
  let head = ranked.next().ok_or(VoiceResolveError::NoMatch)?;
  let rest: Vec<Candidate> = ranked.collect();
  Ok(resolved(head, &rest, None))
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
    tests::{NullObserver, search_response, test_client},
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
      matches!(err, VoiceResolveError::NoMatch),
      "a station needs a seed; there is no bare-station fallback: {err:?}"
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
}

#[cfg(test)]
mod live {
  use std::{path::PathBuf, sync::Arc};

  use super::*;
  use crate::{
    auth::{Auth, DEFAULT_WORKER_BASE},
    client::Observer,
    http::SpHttp,
    httpx::HttpExecutor,
    model::{AuthState, Device, LibraryScope, PlayerState, Queue},
    spclient::SpClient,
    store::{FileTokenStore, load_or_make_device_id},
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

  async fn client() -> SpotifyClient {
    let psk = std::env::var("SPOTIFY_AUTH_PSK").expect("SPOTIFY_AUTH_PSK gates the private-auth worker");
    let base = std::env::var("SPOTIFY_AUTH_BASE").unwrap_or_else(|_| DEFAULT_WORKER_BASE.to_string());
    let dir = state_dir();
    let store = FileTokenStore::new(&dir).expect("private state dir");
    let device_id = load_or_make_device_id(&dir);
    let exec = HttpExecutor::new();
    let auth = Arc::new(Auth::new(base, psk, Box::new(store), exec.clone()));
    assert!(
      auth.is_paired().await,
      "live lane needs a paired refresh token in {}; run `sfp pair` first",
      dir.display()
    );
    let client = SpotifyClient::new(auth, device_id, exec, Arc::new(Silent));
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

  async fn assert_station_resolves(uri: &str) {
    let psk = std::env::var("SPOTIFY_AUTH_PSK").expect("SPOTIFY_AUTH_PSK");
    let base = std::env::var("SPOTIFY_AUTH_BASE").unwrap_or_else(|_| DEFAULT_WORKER_BASE.to_string());
    let dir = state_dir();
    let store = FileTokenStore::new(&dir).expect("private state dir");
    let exec = HttpExecutor::new();
    let auth = Arc::new(Auth::new(base, psk, Box::new(store), exec.clone()));
    let spc = SpClient::new(SpHttp::new(auth, exec));
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
