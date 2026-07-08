//! credit to the librespot project

use std::{
  collections::{HashMap, HashSet},
  sync::Arc,
  time::Duration,
};

use futures::future::join_all;
use librespot_protocol::connect::Cluster;
use serde_json::json;
use tokio::{
  sync::{Mutex, Notify},
  task::JoinHandle,
};

use crate::{
  aplogin,
  auth::{Auth, DeviceFlow, TokenStore},
  dealer::{Dealer, DealerEvent, DealerWriter, active_device},
  error::{Error, Result},
  http::SpHttp,
  httpx::{HttpExecutor, HttpTransport},
  model::{
    self, AuthState, BrowseItem, BrowsePage, Device, LibraryScope, PlayerState, ProductState, Queue, RepeatMode,
    SearchResults, Shelf, Track,
  },
  spclient::SpClient,
  transport::WsTransport,
  util::gid_to_base62,
};

const NODE_RECENTS: &str = "recently-played";
const NODE_PLAYLISTS: &str = "playlists";
const NODE_ALBUMS: &str = "albums";
const NODE_ARTISTS: &str = "artists";
const NODE_PODCASTS: &str = "podcasts";
const PREVIEW: u32 = 14;
const RECENTS_CACHE_TTL: Duration = Duration::from_secs(60);
const HYDRATE_CACHE_CAP: usize = 4096;
const LIBRARY_CHANGE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(750);
const DJ_URI: &str = "spotify:playlist:37i9dQZF1EYkqdzj48dyYq";

#[uniffi::export(callback_interface)]
pub trait Observer: Send + Sync {
  fn on_player(&self, state: PlayerState);
  fn on_queue(&self, queue: Queue);
  fn on_devices(&self, devices: Vec<Device>);
  fn on_auth(&self, state: AuthState);
  fn on_library_changed(&self, scope: LibraryScope);
}

#[uniffi::export(with_foreign)]
pub trait DeviceWaker: Send + Sync {
  fn wake_device(&self);
}

const DEVICE_WAKE_TIMEOUT: Duration = Duration::from_secs(8);

struct Shared {
  writer: Mutex<Option<DealerWriter>>,
  cluster: Mutex<Option<Cluster>>,
  last_active: Mutex<Option<String>>,
  device_waker: std::sync::Mutex<Option<Arc<dyn DeviceWaker>>>,
  cluster_changed: Notify,
}

#[derive(uniffi::Object)]
pub struct SpotifyClient {
  auth: Arc<Auth>,
  http: SpHttp,
  spc: SpClient,
  dealer: Dealer,
  exec: HttpExecutor,
  observer: Arc<dyn Observer>,
  shared: Arc<Shared>,
  username: Mutex<Option<String>>,
  liked: Arc<Mutex<Option<Vec<String>>>>,
  browse_cache: Arc<Mutex<BrowseCache>>,
  loop_handle: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Default)]
struct BrowseCache {
  rootlist: Option<Vec<String>>,
  collections: HashMap<String, Vec<String>>,
  recents: Option<(tokio::time::Instant, Vec<String>)>,
  hydrated: HashMap<String, (u64, BrowseItem)>,
  counter: u64,
}

impl BrowseCache {
  fn note_saved_changed(&mut self) {
    self.collections.clear();
  }

  fn note_playlists_changed(&mut self) {
    self.rootlist = None;
    // playlist hydrations carry mutable name/cover; catalog metadata (tracks/albums/...) is immutable
    self
      .hydrated
      .retain(|uri, _| !uri.starts_with("spotify:playlist:") && !uri.ends_with(":collection"));
  }

  fn hydrated_get(&self, uri: &str) -> Option<BrowseItem> {
    self.hydrated.get(uri).map(|(_, item)| item.clone())
  }

  fn hydrated_put(&mut self, uri: String, item: BrowseItem) {
    self.counter += 1;
    self.hydrated.insert(uri, (self.counter, item));
    if self.hydrated.len() > HYDRATE_CACHE_CAP {
      let floor = self.counter.saturating_sub(HYDRATE_CACHE_CAP as u64);
      self.hydrated.retain(|_, (at, _)| *at > floor);
    }
  }
}

impl SpotifyClient {
  pub fn new(auth: Arc<Auth>, device_id: String, exec: HttpExecutor, observer: Arc<dyn Observer>) -> Self {
    let http = SpHttp::new(auth.clone(), exec.clone());
    let spc = SpClient::new(http.clone());
    let dealer = Dealer::new(http.clone(), device_id);
    SpotifyClient {
      auth,
      http,
      spc,
      dealer,
      exec,
      observer,
      shared: Arc::new(Shared {
        writer: Mutex::new(None),
        cluster: Mutex::new(None),
        last_active: Mutex::new(None),
        device_waker: std::sync::Mutex::new(None),
        cluster_changed: Notify::new(),
      }),
      username: Mutex::new(None),
      liked: Arc::new(Mutex::new(None)),
      browse_cache: Arc::new(Mutex::new(BrowseCache::default())),
      loop_handle: Mutex::new(None),
    }
  }

  fn spawn_events_loop(&self) -> JoinHandle<()> {
    tokio::spawn(events_loop(
      self.dealer.clone(),
      self.spc.clone(),
      self.observer.clone(),
      self.shared.clone(),
      self.liked.clone(),
      self.browse_cache.clone(),
      self.dealer.device_id().to_string(),
    ))
  }

  async fn username(&self) -> Result<String> {
    self.username.lock().await.clone().ok_or(Error::NoUsername)
  }

  async fn writer(&self) -> Result<DealerWriter> {
    self
      .shared
      .writer
      .lock()
      .await
      .clone()
      .ok_or_else(|| Error::other("dealer not connected"))
  }

  async fn target(&self) -> Result<String> {
    let last_active = self.shared.last_active.lock().await.clone();
    let guard = self.shared.cluster.lock().await;
    let cluster = guard.as_ref().ok_or_else(|| Error::other("no cluster yet"))?;
    match active_device(cluster, self.dealer.device_id(), last_active.as_deref()) {
      Some(target) => Ok(target),
      None => {
        tracing::warn!(
          devices = cluster.device.len(),
          "spotify command: no reachable target device (phone spotify likely not an active connect device)"
        );
        Err(Error::other("no reachable target device"))
      }
    }
  }

  async fn target_or_wake(&self) -> Result<String> {
    if let Ok(t) = self.target().await {
      return Ok(t);
    }
    let waker = self.shared.device_waker.lock().unwrap().clone();
    let Some(waker) = waker else {
      return self.target().await;
    };
    tracing::info!("spotify play: no live device; asking platform to wake the phone's spotify");
    waker.wake_device();
    match tokio::time::timeout(DEVICE_WAKE_TIMEOUT, self.await_device()).await {
      Ok(res) => res,
      Err(_) => {
        tracing::warn!("spotify play: no device registered within wake timeout");
        Err(Error::other("no device appeared after wake"))
      }
    }
  }

  async fn await_device(&self) -> Result<String> {
    let notified = self.shared.cluster_changed.notified();
    tokio::pin!(notified);
    loop {
      notified.as_mut().enable();
      if let Ok(t) = self.target().await {
        return Ok(t);
      }
      notified.as_mut().await;
      notified.set(self.shared.cluster_changed.notified());
    }
  }

  async fn album_for_track(&self, uri: &str) -> Option<String> {
    let tracks = self.spc.get_tracks(&[uri.to_string()]).await.ok()?;
    let t = tracks.get(uri)?;
    if t.album.gid().is_empty() {
      None
    } else {
      Some(format!("spotify:album:{}", gid_to_base62(t.album.gid())))
    }
  }

  async fn liked_uris(&self, username: &str) -> Result<Vec<String>> {
    if let Some(cached) = self.liked.lock().await.clone() {
      return Ok(cached);
    }
    let uris = fetch_liked_uris(&self.spc, username).await?;
    *self.liked.lock().await = Some(uris.clone());
    Ok(uris)
  }

  fn spawn_liked_warm(&self, username: String) {
    let spc = self.spc.clone();
    let liked = self.liked.clone();
    tokio::spawn(async move {
      if let Ok(uris) = fetch_liked_uris(&spc, &username).await {
        *liked.lock().await = Some(uris);
      }
    });
  }

  async fn recents_uris(&self) -> Result<Vec<String>> {
    if let Some((at, cached)) = self.browse_cache.lock().await.recents.clone()
      && at.elapsed() < RECENTS_CACHE_TTL
    {
      return Ok(cached);
    }
    let user = self.username().await?;
    let rp = self.spc.recently_played(&user, 50).await?;
    let mut seen = std::collections::HashSet::new();
    let uris: Vec<String> = rp
      .items
      .iter()
      .map(|e| e.track_uri.clone())
      .filter(|u| u.starts_with("spotify:track:") && seen.insert(u.clone()))
      .collect();
    self.browse_cache.lock().await.recents = Some((tokio::time::Instant::now(), uris.clone()));
    Ok(uris)
  }

  async fn playlist_uris(&self) -> Result<Vec<String>> {
    if let Some(cached) = self.browse_cache.lock().await.rootlist.clone() {
      return Ok(cached);
    }
    let user = self.username().await?;
    let rl = self.spc.rootlist(&user).await?;
    let uris: Vec<String> = rl
      .contents
      .items
      .iter()
      .map(|i| i.uri().to_string())
      .filter(|u| u.starts_with("spotify:playlist:"))
      .collect();
    self.browse_cache.lock().await.rootlist = Some(uris.clone());
    Ok(uris)
  }

  async fn collection_uris(&self, set: &str, kind: Option<&str>) -> Result<Vec<String>> {
    let key = format!("{set}:{}", kind.unwrap_or(""));
    if let Some(cached) = self.browse_cache.lock().await.collections.get(&key).cloned() {
      return Ok(cached);
    }
    let user = self.username().await?;
    let mut items = self.spc.collection_paging(&user, set, 500).await?;
    items.sort_by(|a, b| b.added_at.cmp(&a.added_at));
    let uris: Vec<String> = items
      .into_iter()
      .map(|i| i.uri)
      .filter(|u| kind.is_none_or(|k| u.split(':').nth(1) == Some(k)))
      .collect();
    self.browse_cache.lock().await.collections.insert(key, uris.clone());
    Ok(uris)
  }

  async fn hydrate_map(&self, uris: &[String]) -> HashMap<String, BrowseItem> {
    let mut out = HashMap::new();
    let missing: Vec<String> = {
      let cache = self.browse_cache.lock().await;
      uris
        .iter()
        .filter(|u| {
          if let Some(item) = cache.hydrated_get(u) {
            out.insert((*u).clone(), item);
            false
          } else {
            true
          }
        })
        .cloned()
        .collect()
    };
    if missing.is_empty() {
      return out;
    }
    let fetched = self.hydrate_map_uncached(&missing).await;
    let mut cache = self.browse_cache.lock().await;
    for (u, item) in fetched {
      cache.hydrated_put(u.clone(), item.clone());
      out.insert(u, item);
    }
    out
  }

  async fn hydrate_map_uncached(&self, uris: &[String]) -> HashMap<String, BrowseItem> {
    let mut by_kind: HashMap<&str, Vec<String>> = HashMap::new();
    let mut playlists: Vec<String> = Vec::new();
    for u in uris {
      match u.split(':').nth(1) {
        Some("playlist") => playlists.push(u.clone()),
        Some(k) => by_kind.entry(k).or_default().push(u.clone()),
        None => {}
      }
    }
    let empty: Vec<String> = Vec::new();
    let ids = |k: &str| by_kind.get(k).unwrap_or(&empty).clone();
    let (track_ids, album_ids, artist_ids, show_ids, episode_ids) =
      (ids("track"), ids("album"), ids("artist"), ids("show"), ids("episode"));
    let (tracks, albums, artists, shows, episodes, pls) = tokio::join!(
      self.spc.get_tracks(&track_ids),
      self.spc.get_albums(&album_ids),
      self.spc.get_artists(&artist_ids),
      self.spc.get_shows(&show_ids),
      self.spc.get_episodes(&episode_ids),
      self.hydrate_playlists(&playlists),
    );
    let tracks = tracks.unwrap_or_default();
    let albums = albums.unwrap_or_default();
    let artists = artists.unwrap_or_default();
    let shows = shows.unwrap_or_default();
    let episodes = episodes.unwrap_or_default();

    let mut out = HashMap::new();
    for u in uris {
      let item = match u.split(':').nth(1) {
        Some("track") => tracks.get(u).map(|t| model::browse_track(u, t)),
        Some("album") => albums.get(u).map(|a| model::browse_album(u, a)),
        Some("artist") => artists.get(u).map(|a| model::browse_artist(u, a)),
        Some("show") => shows.get(u).map(|s| model::browse_show(u, s)),
        Some("episode") => episodes.get(u).map(|e| model::browse_episode(u, e)),
        Some("playlist") => pls.get(u).cloned(),
        _ if u.ends_with(":collection") => Some(liked_songs_item(u)),
        _ => None,
      };
      if let Some(it) = item {
        out.insert(u.clone(), it);
      }
    }
    out
  }

  async fn hydrate_playlists(&self, uris: &[String]) -> HashMap<String, BrowseItem> {
    let fetches = uris.iter().map(|u| {
      let spc = self.spc.clone();
      let u = u.clone();
      async move {
        let id = u.rsplit(':').next().unwrap_or("").to_string();
        (u, spc.get_playlist(&id, 0, Some(4)).await.ok())
      }
    });
    let mut out = HashMap::new();
    let mut need_cover: Vec<(String, String)> = Vec::new();
    for (u, pl) in join_all(fetches).await {
      let Some(pl) = pl else { continue };
      let img = model::playlist_image_hex(&pl.attributes);
      if img.is_empty()
        && let Some(first) = pl.contents.items.iter().find(|i| i.uri().starts_with("spotify:track:"))
      {
        need_cover.push((u.clone(), first.uri().to_string()));
      }
      out.insert(u.clone(), model::browse_playlist(&u, pl.attributes.name(), &img));
    }
    if !need_cover.is_empty() {
      let track_ids: Vec<String> = need_cover.iter().map(|(_, t)| t.clone()).collect();
      if let Ok(tracks) = self.spc.get_tracks(&track_ids).await {
        for (pl_uri, track_uri) in &need_cover {
          if let Some(t) = tracks.get(track_uri) {
            let cover = crate::util::image_hex(&t.album.cover_group);
            if !cover.is_empty()
              && let Some(item) = out.get_mut(pl_uri)
            {
              item.image_id = cover;
            }
          }
        }
      }
    }
    out
  }

  async fn hydrate_uris(&self, uris: &[String]) -> Vec<BrowseItem> {
    let map = self.hydrate_map(uris).await;
    uris
      .iter()
      .map(|u| {
        map.get(u).cloned().unwrap_or_else(|| BrowseItem {
          uri: u.clone(),
          ..Default::default()
        })
      })
      .collect()
  }

  async fn browse_container(&self, uri: &str, limit: u32, offset: u32) -> Result<BrowsePage> {
    match uri.split(':').nth(1).unwrap_or("") {
      "playlist" => {
        let id = uri.rsplit(':').next().unwrap_or("");
        let pl = self.spc.get_playlist(id, offset, Some(limit)).await?;
        let total = pl.length().max(0) as u32;
        let page: Vec<String> = pl
          .contents
          .items
          .iter()
          .map(|i| i.uri().to_string())
          .filter(|u| u.starts_with("spotify:track:"))
          .collect();
        let count = page.len() as u32;
        let items = self.hydrate_uris(&page).await;
        Ok(BrowsePage {
          items,
          total: Some(total),
          has_more: offset + count < total,
        })
      }
      "album" => {
        let albums = self.spc.get_albums(&[uri.to_string()]).await?;
        let all = albums.get(uri).map(model::album_track_uris).unwrap_or_default();
        self.page_uris(&all, offset, limit).await
      }
      "artist" => {
        let artists = self.spc.get_artists(&[uri.to_string()]).await?;
        let all = artists.get(uri).map(model::artist_top_track_uris).unwrap_or_default();
        self.page_uris(&all, offset, limit).await
      }
      "show" => {
        let shows = self.spc.get_shows(&[uri.to_string()]).await?;
        let all = shows.get(uri).map(model::show_episode_uris).unwrap_or_default();
        self.page_uris(&all, offset, limit).await
      }
      _ if uri.ends_with(":collection") => {
        let user = self.username().await?;
        let all = self.liked_uris(&user).await?;
        self.page_uris(&all, offset, limit).await
      }
      _ => {
        let home = self.spc.get_home("en").await?;
        let all = home
          .body
          .sections
          .iter()
          .find(|s| s.id.uri == uri)
          .map(|s| carousel_of(s).1)
          .unwrap_or_default();
        self.page_uris(&all, offset, limit).await
      }
    }
  }

  async fn page_uris(&self, all: &[String], offset: u32, limit: u32) -> Result<BrowsePage> {
    let total = all.len() as u32;
    let page: Vec<String> = all.iter().skip(offset as usize).take(limit as usize).cloned().collect();
    let count = page.len() as u32;
    let items = self.hydrate_uris(&page).await;
    Ok(BrowsePage {
      items,
      total: Some(total),
      has_more: offset + count < total,
    })
  }

  async fn play_dj(&self) -> Result<()> {
    let writer = self.writer().await?;
    if self.current_context_uri().await.as_deref() == Some(DJ_URI) {
      writer.dj_signal(&self.target().await?).await?;
      return Ok(());
    }
    let cmd = json!({
        "endpoint": "play",
        "context": {
          "uri": DJ_URI,
          "entity_uri": DJ_URI,
          "url": format!("hm://lexicon-session-provider/context-resolve/v2/session?contextUri={DJ_URI}"),
          "metadata": {},
        },
        "play_origin": {"feature_identifier": "harmony", "feature_version": "9.1.52.1394", "referrer_identifier": "home"},
        "prepare_play_options": {"license": "premium"},
        "play_options": {"reason": "interactive", "operation": "replace", "trigger": "immediately"},
    });
    writer.play(&self.target_or_wake().await?, cmd).await?;
    Ok(())
  }

  async fn current_context_uri(&self) -> Option<String> {
    let guard = self.shared.cluster.lock().await;
    let cluster = guard.as_ref()?;
    let uri = &cluster.player_state.context_uri;
    (!uri.is_empty()).then(|| uri.clone())
  }
}

#[uniffi::export(async_runtime = "tokio")]
impl SpotifyClient {
  #[uniffi::constructor]
  pub fn create(
    base: String,
    psk: String,
    device_id: String,
    store: Box<dyn TokenStore>,
    observer: Box<dyn Observer>,
  ) -> Arc<Self> {
    let exec = HttpExecutor::new();
    let auth = Arc::new(Auth::new(base, psk, store, exec.clone()));
    Arc::new(Self::new(auth, device_id, exec, Arc::from(observer)))
  }

  pub fn set_ws_transport(&self, transport: Arc<dyn WsTransport>) {
    self.dealer.set_transport(transport);
  }

  pub fn set_http_transport(&self, transport: Arc<dyn HttpTransport>) {
    self.exec.set(transport);
  }

  pub fn set_device_waker(&self, waker: Arc<dyn DeviceWaker>) {
    *self.shared.device_waker.lock().unwrap() = Some(waker);
  }

  pub async fn connect(&self) -> Result<()> {
    if let Some(prior) = self.loop_handle.lock().await.take()
      && !prior.is_finished()
    {
      prior.abort();
    }

    let mut paired = self.auth.is_paired().await;
    tracing::info!(paired, "spotify connect: starting auth lifecycle");
    if paired {
      match self.auth.bearer().await {
        Ok(_) => {}
        Err(e) if is_auth_terminal(&e) => {
          tracing::warn!(error = %e, "spotify connect: stored token rejected; re-pairing");
          paired = false;
        }
        Err(e) => tracing::warn!(error = %e, "spotify connect: token check failed transiently; proceeding"),
      }
    }
    if !paired {
      tracing::info!("spotify connect: requesting device code from worker");
      let flow = match self.auth.begin_device_flow().await {
        Ok(f) => f,
        Err(e) => {
          tracing::warn!(error = %e, "spotify connect: device-code request failed");
          self.observer.on_auth(AuthState::Failed { reason: e.to_string() });
          return Err(e);
        }
      };
      tracing::info!("spotify connect: device code issued; emitting pending and awaiting approval");
      self.observer.on_auth(AuthState::Pending {
        url: flow.verification_uri.clone(),
        code: flow.user_code.clone(),
      });
      if let Err(e) = self.auth.complete_device_flow(&flow).await {
        tracing::warn!(error = %e, "spotify connect: device flow did not complete");
        self.observer.on_auth(AuthState::Failed { reason: e.to_string() });
        return Err(e);
      }
      tracing::info!("spotify connect: device flow approved");
    }

    let username = match aplogin::resolve_and_cache(self.auth.as_ref(), &self.http, self.dealer.device_id()).await {
      Ok(u) => Some(u),
      Err(e) if is_auth_terminal(&e) => {
        tracing::warn!(error = %e, "spotify connect: terminal auth error resolving username");
        self.observer.on_auth(AuthState::Failed { reason: e.to_string() });
        return Err(e);
      }
      Err(e) => {
        tracing::warn!("username resolution failed, continuing bearer-only: {e}");
        None
      }
    };
    *self.username.lock().await = username.clone();

    if let Ok(p) = self.product().await {
      self.http.set_market(&p.country, &p.catalogue).await;
    }
    if let Some(user) = &username {
      self.spawn_liked_warm(user.clone());
    }

    tracing::info!(
      has_username = username.is_some(),
      "spotify connect: logged in, spawning events loop"
    );
    self.observer.on_auth(AuthState::LoggedIn {
      username: username.unwrap_or_default(),
    });

    *self.loop_handle.lock().await = Some(self.spawn_events_loop());
    Ok(())
  }

  pub async fn resync(&self) {
    let mut guard = self.loop_handle.lock().await;
    if guard.is_none() {
      return;
    }
    tracing::info!("spotify resync: re-establishing dealer on request");
    if let Some(h) = guard.take() {
      h.abort();
    }
    *guard = Some(self.spawn_events_loop());
  }

  pub async fn disconnect(&self) {
    if let Some(h) = self.loop_handle.lock().await.take() {
      h.abort();
    }
    *self.shared.writer.lock().await = None;
    *self.shared.cluster.lock().await = None;
    *self.shared.last_active.lock().await = None;
    *self.browse_cache.lock().await = BrowseCache::default();
  }

  pub async fn current_position_ms(&self) -> Option<u32> {
    let guard = self.shared.cluster.lock().await;
    let cluster = guard.as_ref()?;
    (!cluster.player_state.track.uri.is_empty()).then(|| model::position_now(&cluster.player_state))
  }

  // ---- commands -----------------------------------------------------------

  pub async fn pause(&self) -> Result<()> {
    self.writer().await?.pause(&self.target().await?).await?;
    Ok(())
  }
  pub async fn resume(&self) -> Result<()> {
    self.writer().await?.resume(&self.target_or_wake().await?).await?;
    Ok(())
  }
  pub async fn skip_next(&self) -> Result<()> {
    self.writer().await?.skip_next(&self.target().await?).await?;
    Ok(())
  }
  pub async fn skip_prev(&self) -> Result<()> {
    self.writer().await?.skip_prev(&self.target().await?).await?;
    Ok(())
  }
  pub async fn seek(&self, position_ms: i64) -> Result<()> {
    self.writer().await?.seek_to(&self.target().await?, position_ms).await?;
    Ok(())
  }
  pub async fn set_shuffle(&self, on: bool) -> Result<()> {
    self.writer().await?.set_shuffle(&self.target().await?, on).await?;
    Ok(())
  }
  pub async fn set_repeat(&self, mode: RepeatMode) -> Result<()> {
    let writer = self.writer().await?;
    let target = self.target().await?;
    writer.set_repeat_context(&target, mode == RepeatMode::Context).await?;
    writer.set_repeat_track(&target, mode == RepeatMode::Track).await?;
    Ok(())
  }
  pub async fn set_volume(&self, percent: f64) -> Result<()> {
    self.writer().await?.set_volume(&self.target().await?, percent).await?;
    Ok(())
  }
  pub async fn active_device_volume_percent(&self) -> Option<f64> {
    let guard = self.shared.cluster.lock().await;
    let cluster = guard.as_ref()?;
    if cluster.active_device_id.is_empty() {
      return None;
    }
    let info = cluster.device.get(&cluster.active_device_id)?;
    Some(f64::from(info.volume) / 65535.0 * 100.0)
  }
  pub async fn volume_step(&self, delta_percent: f64) -> Result<f64> {
    let current = self.active_device_volume_percent().await.unwrap_or(50.0);
    let target = (current + delta_percent).clamp(0.0, 100.0);
    self.set_volume(target).await?;
    Ok(target)
  }
  pub async fn queue_uri(&self, uri: &str) -> Result<()> {
    self.writer().await?.add_to_queue(&self.target().await?, uri).await?;
    Ok(())
  }
  pub async fn transfer(&self, device_id: &str) -> Result<()> {
    self.writer().await?.transfer(device_id).await?;
    Ok(())
  }

  pub async fn play(&self, uri: &str, skip_to_uri: Option<String>) -> Result<()> {
    if uri == DJ_URI {
      return self.play_dj().await;
    }
    let writer = self.writer().await?;
    let target = self.target_or_wake().await?;
    let (context, skip) = if uri.starts_with("spotify:track:") && skip_to_uri.is_none() {
      match self.album_for_track(uri).await {
        Some(album) => (album, Some(uri.to_string())),
        None => (uri.to_string(), None),
      }
    } else {
      (uri.to_string(), skip_to_uri)
    };
    let mut ppo = json!({ "license": "premium" });
    if let Some(s) = skip {
      ppo["skip_to"] = json!({ "track_uri": s });
    }
    let cmd = json!({
        "endpoint": "play",
        "context": {"uri": context, "url": format!("context://{context}"), "metadata": {}},
        "play_origin": {"feature_identifier": "harmony", "feature_version": "9.1.52.1394", "referrer_identifier": "home"},
        "prepare_play_options": ppo,
        "play_options": {"reason": "interactive", "operation": "replace", "trigger": "immediately"},
    });
    writer.play(&target, cmd).await?;
    Ok(())
  }

  // ---- content ------------------------------------------------------------

  pub async fn search(&self, query: &str, limit: u32) -> Result<SearchResults> {
    let resp = self.spc.search(query, limit.max(20)).await?;
    let mut out = SearchResults::default();
    for item in flatten_search(&resp) {
      let bucket = match item.uri.split(':').nth(1) {
        Some("track") => &mut out.tracks,
        Some("album") => &mut out.albums,
        Some("artist") => &mut out.artists,
        Some("playlist") => &mut out.playlists,
        _ => continue,
      };
      if bucket.len() < limit as usize {
        bucket.push(BrowseItem {
          uri: item.uri.clone(),
          title: item.name.clone(),
          image_id: model::cdn_image_ref(&item.image),
          playable: true,
          has_children: !item.uri.starts_with("spotify:track:"),
          ..Default::default()
        });
      }
    }
    Ok(out)
  }

  pub async fn product(&self) -> Result<ProductState> {
    let v = self.spc.product_state().await?;
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let product = s("product");
    let catalogue = s("catalogue");
    let is_premium = product == "premium" || catalogue == "premium";
    let country = s("country");
    self.http.set_market(&country, &catalogue).await;
    Ok(ProductState {
      country,
      is_premium,
      can_use_superbird: is_premium,
      product,
      catalogue,
    })
  }

  pub async fn root_browse(&self, sections: Option<u32>, preview: Option<u32>) -> Result<Vec<Shelf>> {
    let preview = preview.unwrap_or(PREVIEW).min(PREVIEW) as usize;
    let user = self.username().await.ok();
    let (home, playlists, albums, artists, shows) = tokio::join!(
      self.spc.get_home("en"),
      self.playlist_uris(),
      self.collection_uris("collection", Some("album")),
      self.collection_uris("artist", None),
      self.collection_uris("show", None),
    );
    let albums = albums.unwrap_or_default();
    let artists = artists.unwrap_or_default();
    let shows = shows.unwrap_or_default();
    let mut playlists_all: Vec<String> = Vec::new();
    if let Some(u) = &user {
      playlists_all.push(format!("spotify:user:{u}:collection"));
    }
    playlists_all.extend(playlists.unwrap_or_default());

    let casita_rows: Vec<(String, String, Vec<String>, usize)> = match home {
      Ok(h) => h
        .body
        .sections
        .iter()
        .filter_map(|s| {
          let (title, uris) = carousel_of(s);
          if uris.is_empty() || title.is_empty() {
            return None;
          }
          let total = uris.len();
          Some((s.id.uri.clone(), title, uris.into_iter().take(preview).collect(), total))
        })
        .collect(),
      Err(_) => Vec::new(),
    };

    let take = |v: &[String]| v.iter().take(preview).cloned().collect::<Vec<_>>();
    let mut rows: Vec<(String, String, Vec<String>, usize)> = vec![
      (
        NODE_PLAYLISTS.into(),
        "Playlists".into(),
        take(&playlists_all),
        playlists_all.len(),
      ),
      (NODE_ALBUMS.into(), "Albums".into(), take(&albums), albums.len()),
      (NODE_ARTISTS.into(), "Artists".into(), take(&artists), artists.len()),
      (NODE_PODCASTS.into(), "Podcasts".into(), take(&shows), shows.len()),
    ];
    rows.extend(casita_rows);
    rows.retain(|(_, _, _, total)| *total > 0);
    if let Some(cap) = sections {
      rows.truncate(cap as usize);
    }

    if preview == 0 {
      return Ok(
        rows
          .into_iter()
          .map(|(id, title, _, total)| Shelf {
            id,
            title,
            items: Vec::new(),
            total: total as u32,
          })
          .collect(),
      );
    }

    let mut union: Vec<String> = Vec::new();
    for (_, _, uris, _) in &rows {
      union.extend(uris.iter().cloned());
    }
    let mut seen = std::collections::HashSet::new();
    union.retain(|u| seen.insert(u.clone()));
    let map = self.hydrate_map(&union).await;

    Ok(
      rows
        .into_iter()
        .filter_map(|(id, title, uris, total)| {
          let items: Vec<BrowseItem> = uris
            .iter()
            .filter_map(|u| map.get(u).cloned())
            .filter(|i| !i.title.is_empty())
            .collect();
          if items.is_empty() {
            return None;
          }
          Some(Shelf {
            id,
            title,
            items,
            total: total as u32,
          })
        })
        .collect(),
    )
  }

  pub async fn browse(&self, node_id: &str, limit: u32, offset: u32) -> Result<BrowsePage> {
    let all: Vec<String> = match node_id {
      NODE_RECENTS => self.recents_uris().await?,
      NODE_PLAYLISTS => {
        let mut u = Vec::new();
        if let Ok(user) = self.username().await {
          u.push(format!("spotify:user:{user}:collection"));
        }
        u.extend(self.playlist_uris().await.unwrap_or_default());
        u
      }
      NODE_ALBUMS => self.collection_uris("collection", Some("album")).await?,
      NODE_ARTISTS => self.collection_uris("artist", None).await?,
      NODE_PODCASTS => self.collection_uris("show", None).await?,
      _ => return self.browse_container(node_id, limit, offset).await,
    };
    self.page_uris(&all, offset, limit).await
  }

  pub async fn resolve_context(&self, uri: &str) -> Result<BrowseItem> {
    Ok(
      self
        .hydrate_uris(std::slice::from_ref(&uri.to_string()))
        .await
        .into_iter()
        .next()
        .unwrap_or_default(),
    )
  }

  pub async fn favorites_list(&self, limit: u32, offset: u32) -> Result<BrowsePage> {
    let user = self.username().await?;
    let all = self.liked_uris(&user).await?;
    let mut page = self.page_uris(&all, offset, limit).await?;
    for it in page.items.iter_mut() {
      it.saved = true;
    }
    Ok(page)
  }

  // ---- favorites ----------------------------------------------------------

  pub async fn favorites_contains(&self, uris: Vec<String>) -> Result<Vec<bool>> {
    let user = self.username().await?;
    let liked = self.liked_uris(&user).await?;
    let set: std::collections::HashSet<&String> = liked.iter().collect();
    Ok(uris.iter().map(|u| set.contains(u)).collect())
  }

  pub async fn favorites_set(&self, uri: &str, liked: bool) -> Result<()> {
    let user = self.username().await?;
    let one = [uri.to_string()];
    if liked {
      self.spc.collection_write(&user, "collection", &one, &[]).await?;
    } else {
      self.spc.collection_write(&user, "collection", &[], &one).await?;
    }
    let mut guard = self.liked.lock().await;
    if let Some(cache) = guard.as_mut() {
      cache.retain(|u| u != uri);
      if liked {
        cache.insert(0, uri.to_string());
      }
    }
    drop(guard);
    self.browse_cache.lock().await.note_saved_changed();
    Ok(())
  }

  // ---- pairing ------------------------------------------------------------

  pub async fn begin_device_flow(&self) -> Result<DeviceFlow> {
    self.auth.begin_device_flow().await
  }

  pub async fn complete_device_flow(&self, flow: DeviceFlow) -> Result<()> {
    self.auth.complete_device_flow(&flow).await
  }
}

const LIKED_SONGS_ART_REF: &str = "builtin:liked-songs";

fn liked_songs_item(uri: &str) -> BrowseItem {
  BrowseItem {
    uri: uri.to_string(),
    title: "Liked Songs".to_string(),
    subtitle: "Playlist".to_string(),
    image_id: LIKED_SONGS_ART_REF.to_string(),
    playable: true,
    has_children: true,
    ..Default::default()
  }
}

fn carousel_of(section: &crate::proto::custom::casita_home::Section) -> (String, Vec<String>) {
  for car in [&section.shortcuts, &section.carousel, &section.list_carousel] {
    if let Some(c) = car.as_ref() {
      let uris: Vec<String> = c.items.inner.items.iter().map(|i| i.uri.clone()).collect();
      if !uris.is_empty() {
        return (c.header.title.text.clone(), uris);
      }
    }
  }
  (String::new(), Vec::new())
}

struct FlatItem {
  uri: String,
  name: String,
  image: String,
}

fn flatten_search(resp: &crate::proto::custom::searchview::SearchResponse) -> Vec<FlatItem> {
  let mut out = Vec::new();
  let mut seen = std::collections::HashSet::new();
  let mut push = |uri: &str, name: &str, image: &str, out: &mut Vec<FlatItem>| {
    if !uri.is_empty() && seen.insert(uri.to_string()) {
      out.push(FlatItem {
        uri: uri.to_string(),
        name: name.to_string(),
        image: image.to_string(),
      });
    }
  };
  for it in &resp.items {
    if let Some(section) = it.section.as_ref() {
      for entry in &section.entries {
        let e = &entry.item.entity;
        push(&e.uri, &e.name, &e.image, &mut out);
      }
    } else if !it.uri.is_empty() {
      push(&it.uri, &it.name, &it.image, &mut out);
    }
  }
  out
}

fn is_auth_terminal(e: &Error) -> bool {
  matches!(e, Error::InvalidGrant | Error::NotPaired)
}

async fn fetch_liked_uris(spc: &SpClient, username: &str) -> Result<Vec<String>> {
  let items = spc.collection_paging(username, "collection", 1000).await?;
  Ok(
    items
      .into_iter()
      .map(|i| i.uri)
      .filter(|u| u.starts_with("spotify:track:"))
      .collect(),
  )
}

async fn resolve_context_name(spc: &SpClient, uri: &str) -> Option<String> {
  match uri.split(':').nth(1) {
    Some("playlist") => {
      let id = uri.rsplit(':').next()?;
      let pl = spc.get_playlist(id, 0, Some(1)).await.ok()?;
      let name = pl.attributes.name();
      (!name.is_empty()).then(|| name.to_string())
    }
    Some("album") => {
      let map = spc.get_albums(std::slice::from_ref(&uri.to_string())).await.ok()?;
      map.get(uri).map(|a| a.name().to_string())
    }
    Some("artist") => {
      let map = spc.get_artists(std::slice::from_ref(&uri.to_string())).await.ok()?;
      map.get(uri).map(|a| a.name().to_string())
    }
    _ if uri.ends_with(":collection") => Some("Liked Songs".to_string()),
    _ => None,
  }
}

async fn events_loop(
  dealer: Dealer,
  spc: SpClient,
  observer: Arc<dyn Observer>,
  shared: Arc<Shared>,
  liked: Arc<Mutex<Option<Vec<String>>>>,
  browse_cache: Arc<Mutex<BrowseCache>>,
  me: String,
) {
  loop {
    match dealer.open().await {
      Ok((mut stream, writer)) => match writer.cluster().await {
        Ok(cluster) => {
          tracing::info!(
            active_device = %cluster.active_device_id,
            devices = cluster.device.len(),
            "dealer connected"
          );
          *browse_cache.lock().await = BrowseCache::default();
          *shared.writer.lock().await = Some(writer);
          let mut emitter = Emitter {
            spc: &spc,
            observer: &observer,
            liked: &liked,
            me: &me,
            last_np: String::new(),
            last_q: String::new(),
            last_dev: String::new(),
            hydrated: None,
            q_hydrate: HashMap::new(),
            ctx_names: HashMap::new(),
          };
          emitter.emit(&cluster, true).await;
          if !cluster.active_device_id.is_empty() {
            *shared.last_active.lock().await = Some(cluster.active_device_id.clone());
          }
          *shared.cluster.lock().await = Some(cluster);
          shared.cluster_changed.notify_waiters();
          let mut pending_saved = false;
          let mut pending_playlists = false;
          let debounce = tokio::time::sleep(LIBRARY_CHANGE_DEBOUNCE);
          tokio::pin!(debounce);
          loop {
            tokio::select! {
                event = stream.next_event() => match event {
                  Ok(Some(DealerEvent::Cluster(cluster))) => {
                    emitter.emit(&cluster, false).await;
                    if !cluster.active_device_id.is_empty() {
                      *shared.last_active.lock().await = Some(cluster.active_device_id.clone());
                    }
                    *shared.cluster.lock().await = Some(cluster);
            shared.cluster_changed.notify_waiters();
                  }
                  Ok(Some(DealerEvent::LibraryChanged(scope))) => {
                    match scope {
                      LibraryScope::Saved => {
                        *liked.lock().await = None;
                        browse_cache.lock().await.note_saved_changed();
                        pending_saved = true;
                      }
                      LibraryScope::Playlists => {
                        browse_cache.lock().await.note_playlists_changed();
                        pending_playlists = true;
                      }
                    }
                    debounce.as_mut().reset(tokio::time::Instant::now() + LIBRARY_CHANGE_DEBOUNCE);
                  }
                  Ok(None) => break,
                  Err(e) => {
                    tracing::warn!("dealer read error: {e}");
                    break;
                  }
                },
                _ = &mut debounce, if pending_saved || pending_playlists => {
                  if std::mem::take(&mut pending_saved) {
                    observer.on_library_changed(LibraryScope::Saved);
                  }
                  if std::mem::take(&mut pending_playlists) {
                    observer.on_library_changed(LibraryScope::Playlists);
                  }
                }
              }
          }
          if std::mem::take(&mut pending_saved) {
            observer.on_library_changed(LibraryScope::Saved);
          }
          if std::mem::take(&mut pending_playlists) {
            observer.on_library_changed(LibraryScope::Playlists);
          }
        }
        Err(e) if is_auth_terminal(&e) => {
          observer.on_auth(AuthState::Failed { reason: e.to_string() });
          return;
        }
        Err(e) => tracing::warn!("cluster register failed: {e}"),
      },
      Err(e) if is_auth_terminal(&e) => {
        observer.on_auth(AuthState::Failed { reason: e.to_string() });
        return;
      }
      Err(e) => tracing::warn!("dealer open failed: {e}"),
    }
    *shared.writer.lock().await = None;
    tokio::time::sleep(Duration::from_secs(2)).await;
  }
}

struct Emitter<'a> {
  spc: &'a SpClient,
  observer: &'a Arc<dyn Observer>,
  liked: &'a Mutex<Option<Vec<String>>>,
  me: &'a str,
  last_np: String,
  last_q: String,
  last_dev: String,
  hydrated: Option<(String, Track)>,
  q_hydrate: HashMap<String, Track>,
  ctx_names: HashMap<String, String>,
}

impl Emitter<'_> {
  async fn emit(&mut self, cluster: &Cluster, force: bool) {
    let ps = &cluster.player_state;
    let o = &ps.options;
    let r = &ps.restrictions;
    let np_sig = format!(
      "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
      ps.track.uri,
      ps.is_paused,
      ps.position_as_of_timestamp,
      cluster.active_device_id,
      o.shuffling_context,
      o.repeating_context,
      o.repeating_track,
      r.disallow_seeking_reasons.is_empty(),
      r.disallow_skipping_next_reasons.is_empty(),
      r.disallow_skipping_prev_reasons.is_empty(),
      r.disallow_toggling_shuffle_reasons.is_empty(),
      r.disallow_toggling_repeat_context_reasons.is_empty(),
      r.disallow_toggling_repeat_track_reasons.is_empty(),
    );
    if force || np_sig != self.last_np {
      self.last_np = np_sig;
      let mut state = model::player_state(cluster);
      if let Some(track) = state.track.as_mut() {
        self.hydrate_track(track).await;
        self.fill_saved(track).await;
        if state.duration_ms == 0 {
          state.duration_ms = track.duration_ms;
        }
      }
      if state.context_name.is_empty()
        && !state.context_uri.is_empty()
        && let Some(name) = self.context_name(&state.context_uri).await
      {
        state.context_name = name;
      }
      self.observer.on_player(state);
    }

    let dev_sig = cluster
      .device
      .iter()
      .map(|(id, info)| format!("{id}:{}:{}", info.volume, *id == cluster.active_device_id))
      .collect::<Vec<_>>()
      .join(",");
    if force || dev_sig != self.last_dev {
      self.last_dev = dev_sig;
      self.observer.on_devices(model::devices(cluster, self.me));
    }

    let q_sig = ps
      .next_tracks
      .iter()
      .map(|t| t.uri.as_str())
      .collect::<Vec<_>>()
      .join(",");
    if force || q_sig != self.last_q {
      self.last_q = q_sig;
      let mut queue = model::queue(cluster);
      self.hydrate_queue(&mut queue).await;
      self.observer.on_queue(queue);
    }
  }

  async fn hydrate_track(&mut self, track: &mut Track) {
    if !(track.uri.starts_with("spotify:track:") && (track.artists.is_empty() || track.duration_ms == 0)) {
      return;
    }
    if let Some((uri, cached)) = &self.hydrated
      && uri == &track.uri
    {
      model::fill_track_from_cached(track, cached);
      return;
    }
    if let Ok(map) = self.spc.get_tracks(std::slice::from_ref(&track.uri)).await
      && let Some(t) = map.get(&track.uri)
    {
      model::fill_track_from_proto(track, t);
      self.hydrated = Some((track.uri.clone(), track.clone()));
    }
  }

  async fn hydrate_queue(&mut self, q: &mut Queue) {
    let need: Vec<String> = q
      .next
      .iter()
      .filter(|t| t.uri.starts_with("spotify:track:") && !self.q_hydrate.contains_key(&t.uri))
      .map(|t| t.uri.clone())
      .collect();
    if !need.is_empty()
      && let Ok(map) = self.spc.get_tracks(&need).await
    {
      for t in &q.next {
        if self.q_hydrate.contains_key(&t.uri) {
          continue;
        }
        if let Some(proto) = map.get(&t.uri) {
          let mut filled = t.clone();
          model::fill_track_from_proto(&mut filled, proto);
          self.q_hydrate.insert(t.uri.clone(), filled);
        }
      }
    }
    for t in q.next.iter_mut() {
      if let Some(cached) = self.q_hydrate.get(&t.uri) {
        model::fill_track_from_cached(t, cached);
      }
    }
    let live: HashSet<&str> = q.next.iter().map(|t| t.uri.as_str()).collect();
    self.q_hydrate.retain(|k, _| live.contains(k.as_str()));
  }

  async fn fill_saved(&self, track: &mut Track) {
    if track.saved {
      return;
    }
    if let Some(cache) = self.liked.lock().await.as_ref() {
      track.saved = cache.iter().any(|u| u == &track.uri);
    }
  }

  async fn context_name(&mut self, uri: &str) -> Option<String> {
    if let Some(name) = self.ctx_names.get(uri) {
      return Some(name.clone());
    }
    let name = resolve_context_name(self.spc, uri).await?;
    self.ctx_names.insert(uri.to_string(), name.clone());
    Some(name)
  }
}

#[cfg(test)]
mod tests {
  use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
  };

  use librespot_protocol::connect::Cluster;

  use super::*;

  struct NullStore;
  impl TokenStore for NullStore {
    fn load_refresh_token(&self) -> Option<String> {
      None
    }
    fn save_refresh_token(&self, _token: String) {}
    fn load_username(&self) -> Option<String> {
      None
    }
    fn save_username(&self, _username: String) {}
  }

  struct NullObserver;
  impl Observer for NullObserver {
    fn on_player(&self, _state: PlayerState) {}
    fn on_queue(&self, _queue: Queue) {}
    fn on_devices(&self, _devices: Vec<Device>) {}
    fn on_auth(&self, _state: AuthState) {}
    fn on_library_changed(&self, _scope: LibraryScope) {}
  }

  // records wake calls; optionally injects a device into the cluster to simulate the phone's spotify
  // registering after being woken.
  struct FakeWaker {
    calls: Arc<AtomicUsize>,
    inject: Option<Arc<Shared>>,
  }
  impl DeviceWaker for FakeWaker {
    fn wake_device(&self) {
      self.calls.fetch_add(1, Ordering::SeqCst);
      if let Some(shared) = self.inject.clone() {
        tokio::spawn(async move {
          *shared.cluster.lock().await = Some(active_cluster("dev1"));
          shared.cluster_changed.notify_waiters();
        });
      }
    }
  }

  fn active_cluster(id: &str) -> Cluster {
    librespot_protocol::connect::Cluster {
      active_device_id: id.to_string(),
      ..Default::default()
    }
  }

  fn test_client(observer: Arc<dyn Observer>) -> SpotifyClient {
    let exec = HttpExecutor::new();
    let auth = Arc::new(Auth::new(
      "https://example.invalid",
      "psk",
      Box::new(NullStore),
      exec.clone(),
    ));
    SpotifyClient::new(auth, "me-device".to_string(), exec, observer)
  }

  #[tokio::test]
  async fn target_or_wake_returns_existing_device_without_waking() {
    let client = test_client(Arc::new(NullObserver));
    *client.shared.cluster.lock().await = Some(active_cluster("dev1"));
    let calls = Arc::new(AtomicUsize::new(0));
    client.set_device_waker(Arc::new(FakeWaker {
      calls: calls.clone(),
      inject: None,
    }));

    let target = client.target_or_wake().await.unwrap();
    assert_eq!(target, "dev1");
    assert_eq!(calls.load(Ordering::SeqCst), 0, "no wake when a device already exists");
  }

  #[tokio::test]
  async fn target_or_wake_wakes_and_resolves_when_device_registers() {
    let client = test_client(Arc::new(NullObserver));
    let calls = Arc::new(AtomicUsize::new(0));
    client.set_device_waker(Arc::new(FakeWaker {
      calls: calls.clone(),
      inject: Some(client.shared.clone()),
    }));

    let target = client.target_or_wake().await.unwrap();
    assert_eq!(target, "dev1");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "woke exactly once");
  }
}
