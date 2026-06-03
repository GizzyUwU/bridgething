use std::{
  collections::{HashMap, HashSet},
  num::NonZeroUsize,
  time::{Duration, Instant},
};

use libbridgething::{
  CompanionAuthorityScope, MediaItem, MediaItemUpdate, NowPlayingUpdate, Playback, PlaybackOptions, PlaybackState,
  PlaybackUpdate, PlayerOptions, PlayerState as WirePlayerState, QueueItem, RepeatMode, Track,
  client::{PlayerQueueReply, PlayerStateReply},
  gateway::{NowPlayingEnrichment, QueueSnapshot},
};
use lru::LruCache;

use crate::authority::AuthorityRegistry;

const TRANSPORT_INTENT_WINDOW: Duration = Duration::from_millis(1500);
const SEEK_INTENT_WINDOW: Duration = Duration::from_millis(1500);

const ENRICHMENT_CACHE_CAP: usize = 32;
const DURATION_TOLERANCE_MS: u32 = 3000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NowPlayingSource {
  Iap2,
  Companion,
}

#[derive(Debug, Clone)]
pub struct PlayerState {
  authority: AuthorityRegistry,

  pub playing: bool,

  pub context_title: String,
  pub context_id: Option<String>,

  position_anchor: Option<Instant>,
  pub position_ms: usize,
  pub playback_speed: f64,

  pub track: Option<Track>,

  pub options: PlaybackOptions,

  iap2_metadata: MediaItemUpdate,
  iap2_playback: PlaybackUpdate,
  companion_metadata: MediaItemUpdate,
  companion_playback: PlaybackUpdate,

  iap2_queue: Vec<QueueItem>,
  companion_queue: Vec<QueueItem>,

  enrichment: Option<NowPlayingEnrichment>,
  enrichment_by_pid: LruCache<String, EnrichedEntry>,

  present_ids: HashSet<String>,

  transport_intent: Option<TransportIntent>,
  seek_intent: Option<SeekIntent>,
}

#[derive(Debug, Clone, Copy)]
struct TransportIntent {
  playing: bool,
  expires: Instant,
}

#[derive(Debug, Clone, Copy)]
struct SeekIntent {
  expires: Instant,
}

impl PlayerState {
  pub fn new(authority: AuthorityRegistry) -> Self {
    Self {
      authority,

      playing: false,

      context_title: String::new(),
      context_id: None,

      position_ms: 0,
      position_anchor: None,
      playback_speed: 1.0,

      track: None,

      options: PlaybackOptions::default(),

      iap2_metadata: MediaItemUpdate::default(),
      iap2_playback: PlaybackUpdate::default(),
      companion_metadata: MediaItemUpdate::default(),
      companion_playback: PlaybackUpdate::default(),

      iap2_queue: Vec::new(),
      companion_queue: Vec::new(),

      enrichment: None,
      enrichment_by_pid: LruCache::new(NonZeroUsize::new(ENRICHMENT_CACHE_CAP).unwrap()),

      present_ids: HashSet::new(),

      transport_intent: None,
      seek_intent: None,
    }
  }

  pub(crate) fn is_present(&self, id: &str) -> bool {
    self.present_ids.contains(id)
  }

  pub(crate) fn note_asset_ready(&mut self, id: String) {
    self.present_ids.insert(id);
  }

  pub(crate) fn note_asset_cleared(&mut self, id: &str) {
    self.present_ids.remove(id);
  }

  fn gate_head_art(&self, merged: &MediaItemUpdate, overlay: &Overlay) -> Option<String> {
    let spotify = match overlay.tier {
      Tier::Bare => None,
      _ => overlay.art.clone(),
    }
    .filter(|s| !s.is_empty());
    let fallback = merged.artwork_id.clone().filter(|s| !s.is_empty());

    if let Some(sp) = &spotify
      && self.present_ids.contains(sp)
    {
      return spotify;
    }
    if let Some(fb) = &fallback
      && self.present_ids.contains(fb)
    {
      return fallback;
    }
    spotify.or(fallback)
  }

  pub(crate) fn set_transport_intent(&mut self, playing: bool) {
    self.position_ms = self.current_position_ms();
    self.position_anchor = Some(Instant::now());
    self.playing = playing;
    self.transport_intent = Some(TransportIntent {
      playing,
      expires: Instant::now() + TRANSPORT_INTENT_WINDOW,
    });
  }

  fn active_transport_intent(&self) -> Option<bool> {
    let intent = self.transport_intent?;
    (Instant::now() < intent.expires).then_some(intent.playing)
  }

  pub(crate) fn set_seek_intent(&mut self, position_ms: u32) {
    self.position_ms = position_ms as usize;
    self.position_anchor = Some(Instant::now());
    self.seek_intent = Some(SeekIntent {
      expires: Instant::now() + SEEK_INTENT_WINDOW,
    });
  }

  fn current_position_ms(&self) -> usize {
    if !self.playing {
      return self.position_ms;
    }
    let Some(anchor) = self.position_anchor else {
      return self.position_ms;
    };
    let elapsed = Instant::now().saturating_duration_since(anchor).as_millis() as usize;
    self.position_ms.saturating_add(elapsed)
  }

  fn seek_intent_active(&self) -> bool {
    self.seek_intent.is_some_and(|i| Instant::now() < i.expires)
  }

  pub(crate) fn replace_iap2_queue(&mut self, items: Vec<QueueItem>) {
    self.iap2_queue = items;
  }

  pub(crate) fn apply_companion_queue(&mut self, snapshot: QueueSnapshot) {
    let QueueSnapshot { order, items } = snapshot;
    let mut by_uri: HashMap<String, QueueItem> =
      self.companion_queue.drain(..).map(|q| (q.uri.clone(), q)).collect();
    for item in items {
      by_uri.insert(item.uri.clone(), item);
    }
    let mut rebuilt = Vec::with_capacity(order.len());
    for uri in &order {
      if let Some(item) = by_uri.get(uri) {
        rebuilt.push(item.clone());
      }
    }
    if rebuilt.len() != order.len() {
      tracing::warn!(
        ordered = order.len(),
        resolved = rebuilt.len(),
        "companion queue: ordered uris without a cached item were dropped"
      );
    }
    self.companion_queue = rebuilt;
  }

  pub(crate) fn apply_companion_snapshot(&mut self, snapshot: WirePlayerState) {
    let WirePlayerState {
      track,
      playback,
      queue,
      options,
    } = snapshot;

    self.companion_metadata = match track {
      Some(t) => MediaItemUpdate {
        persistent_id: t.persistent_id,
        title: t.title,
        album: t.album,
        album_artist: t.album_artist,
        artist: t.artist,
        liked: t.liked,
        artwork_id: t.artwork_id,
        duration_ms: t.duration_ms,
        media_types: t.media_types,
        track_number: t.track_number,
        track_count: t.track_count,
        is_like_supported: t.is_like_supported,
        is_ban_supported: t.is_ban_supported,
        is_banned: t.is_banned,
        is_resident_on_device: None,
        chapter_count: t.chapter_count,
      },
      None => MediaItemUpdate::default(),
    };

    self.companion_playback = PlaybackUpdate {
      playing: Some(matches!(playback.state, PlaybackState::Playing)),
      position_ms: Some(playback.position_ms),
      shuffle: Some(playback.shuffle),
      shuffle_mode: playback.shuffle_mode,
      repeat: Some(playback.repeat),
      app_bundle: None,
      app_display_name: None,
      queue_index: playback.queue_index,
      queue_count: playback.queue_count,
      queue_chapter_index: playback.queue_chapter_index,
      playback_speed: Some(options.speed),
      set_elapsed_time_available: playback.set_elapsed_time_available,
      queue_list_avail: playback.queue_list_avail,
      apple_music_radio_ad: playback.apple_music_radio_ad,
      apple_music_radio_station_name: None,
    };

    self.companion_queue = queue;

    let merged_meta = self.merged_metadata();
    let merged_play = self.merged_playback();
    self.apply_merged(merged_meta, merged_play);
  }

  fn merged_queue(&self, overlay: &Overlay) -> Vec<QueueItem> {
    let companion_authoritative = self
      .authority
      .is_authoritative(CompanionAuthorityScope::NowPlayingMetadata);
    if companion_authoritative {
      self.companion_queue.clone()
    } else if let Some(queue) = &overlay.queue {
      queue.clone()
    } else {
      self.iap2_queue.clone()
    }
  }

  pub(crate) fn apply_enrichment(&mut self, offer: NowPlayingEnrichment) {
    if let (Some(pid), Some(head)) = (offer.anchor_pid.as_deref(), offer.head.as_ref())
      && !head.uri.is_empty()
    {
      self
        .enrichment_by_pid
        .put(pid.to_string(), EnrichedEntry::from_head(head));
    }
    self.enrichment = Some(offer);
  }

  fn resolve_overlay(&self, id: &MediaItemUpdate) -> Overlay {
    let pid = id.persistent_id.as_deref();

    if let Some(pid) = pid
      && let Some(entry) = self.enrichment_by_pid.peek(pid)
      && content_agrees(entry, id)
    {
      let anchor_is_current = self.enrichment.as_ref().and_then(|o| o.anchor_pid.as_deref()) == Some(pid);
      return Overlay {
        tier: Tier::Exact,
        art: entry.artwork_id.clone(),
        uri: Some(entry.uri.clone()),
        duration_ms: entry.duration_ms,
        like_supported: true,
        queue: (anchor_is_current && !self.companion_queue.is_empty()).then(|| self.companion_queue.clone()),
      };
    }

    let candidates = self
      .enrichment
      .iter()
      .flat_map(|o| o.head.iter())
      .chain(self.companion_queue.iter())
      .map(Cand::from_queue)
      .chain(self.enrichment_by_pid.iter().map(|(_, e)| Cand::from_entry(e)));
    if let Some(cand) = best_content_match(id, candidates) {
      return Overlay {
        tier: Tier::Content,
        art: cand.artwork_id,
        duration_ms: cand.duration_ms,
        ..Overlay::bare()
      };
    }

    Overlay::bare()
  }

  pub(crate) fn apply_now_playing(&mut self, source: NowPlayingSource, update: NowPlayingUpdate) {
    let NowPlayingUpdate { media_item, playback } = update;

    let (meta_target, play_target) = match source {
      NowPlayingSource::Companion => (&mut self.companion_metadata, &mut self.companion_playback),
      NowPlayingSource::Iap2 => (&mut self.iap2_metadata, &mut self.iap2_playback),
    };

    if let Some(media) = media_item {
      if let Some(ref new_pid) = media.persistent_id
        && meta_target.persistent_id.as_ref() != Some(new_pid)
      {
        *meta_target = MediaItemUpdate::default();
        play_target.position_ms = Some(0);
      }
      accumulate_media(meta_target, media);
    }
    if let Some(play) = playback {
      accumulate_playback(play_target, play);
    }

    let merged_meta = self.merged_metadata();
    let merged_play = self.merged_playback();

    self.apply_merged(merged_meta, merged_play);
  }

  pub(crate) fn apply_artwork_id(&mut self, source: NowPlayingSource, asset_id: String) {
    let meta_target = match source {
      NowPlayingSource::Companion => &mut self.companion_metadata,
      NowPlayingSource::Iap2 => &mut self.iap2_metadata,
    };
    meta_target.artwork_id = Some(asset_id);

    let merged_meta = self.merged_metadata();
    let merged_play = self.merged_playback();
    self.apply_merged(merged_meta, merged_play);
  }

  fn merged_metadata(&self) -> MediaItemUpdate {
    let companion_authoritative = self
      .authority
      .is_authoritative(CompanionAuthorityScope::NowPlayingMetadata);
    if companion_authoritative {
      MediaItemUpdate {
        persistent_id: self
          .companion_metadata
          .persistent_id
          .clone()
          .or_else(|| self.iap2_metadata.persistent_id.clone()),
        title: self
          .companion_metadata
          .title
          .clone()
          .or_else(|| self.iap2_metadata.title.clone()),
        album: self
          .companion_metadata
          .album
          .clone()
          .or_else(|| self.iap2_metadata.album.clone()),
        album_artist: self
          .companion_metadata
          .album_artist
          .clone()
          .or_else(|| self.iap2_metadata.album_artist.clone()),
        artist: self
          .companion_metadata
          .artist
          .clone()
          .or_else(|| self.iap2_metadata.artist.clone()),
        liked: self.companion_metadata.liked.or(self.iap2_metadata.liked),
        artwork_id: self.companion_metadata.artwork_id.clone(),
        duration_ms: self.companion_metadata.duration_ms.or(self.iap2_metadata.duration_ms),
        media_types: self
          .companion_metadata
          .media_types
          .clone()
          .or_else(|| self.iap2_metadata.media_types.clone()),
        track_number: self.companion_metadata.track_number.or(self.iap2_metadata.track_number),
        track_count: self.companion_metadata.track_count.or(self.iap2_metadata.track_count),
        is_like_supported: self
          .companion_metadata
          .is_like_supported
          .or(self.iap2_metadata.is_like_supported),
        is_ban_supported: self
          .companion_metadata
          .is_ban_supported
          .or(self.iap2_metadata.is_ban_supported),
        is_banned: self.companion_metadata.is_banned.or(self.iap2_metadata.is_banned),
        is_resident_on_device: self
          .companion_metadata
          .is_resident_on_device
          .or(self.iap2_metadata.is_resident_on_device),
        chapter_count: self
          .companion_metadata
          .chapter_count
          .or(self.iap2_metadata.chapter_count),
      }
    } else {
      self.iap2_metadata.clone()
    }
  }

  fn merged_playback(&self) -> PlaybackUpdate {
    let companion_authoritative = self
      .authority
      .is_authoritative(CompanionAuthorityScope::NowPlayingPlayback);
    if companion_authoritative {
      PlaybackUpdate {
        playing: self.companion_playback.playing.or(self.iap2_playback.playing),
        position_ms: self.companion_playback.position_ms.or(self.iap2_playback.position_ms),
        shuffle: self.companion_playback.shuffle.or(self.iap2_playback.shuffle),
        shuffle_mode: self.companion_playback.shuffle_mode.or(self.iap2_playback.shuffle_mode),
        repeat: self.companion_playback.repeat.or(self.iap2_playback.repeat),
        app_bundle: self
          .companion_playback
          .app_bundle
          .clone()
          .or_else(|| self.iap2_playback.app_bundle.clone()),
        app_display_name: self
          .companion_playback
          .app_display_name
          .clone()
          .or_else(|| self.iap2_playback.app_display_name.clone()),
        queue_index: self.companion_playback.queue_index.or(self.iap2_playback.queue_index),
        queue_count: self.companion_playback.queue_count.or(self.iap2_playback.queue_count),
        queue_chapter_index: self
          .companion_playback
          .queue_chapter_index
          .or(self.iap2_playback.queue_chapter_index),
        playback_speed: self
          .companion_playback
          .playback_speed
          .or(self.iap2_playback.playback_speed),
        set_elapsed_time_available: self
          .companion_playback
          .set_elapsed_time_available
          .or(self.iap2_playback.set_elapsed_time_available),
        queue_list_avail: self
          .companion_playback
          .queue_list_avail
          .or(self.iap2_playback.queue_list_avail),
        apple_music_radio_ad: self
          .companion_playback
          .apple_music_radio_ad
          .or(self.iap2_playback.apple_music_radio_ad),
        apple_music_radio_station_name: self
          .companion_playback
          .apple_music_radio_station_name
          .clone()
          .or_else(|| self.iap2_playback.apple_music_radio_station_name.clone()),
      }
    } else {
      self.iap2_playback.clone()
    }
  }

  fn apply_merged(&mut self, media: MediaItemUpdate, playback: PlaybackUpdate) {
    let same_track = match (
      self.track.as_ref().map(|t| t.id.as_str()),
      media.persistent_id.as_deref(),
    ) {
      (Some(existing), Some(new)) => existing == new,
      (None, _) | (_, None) => false,
    };
    let mut track = if same_track {
      self.track.clone().unwrap_or_default()
    } else {
      Track::default()
    };

    if let Some(id) = media.persistent_id {
      track.id = id;
    }
    if let Some(title) = media.title {
      track.name = title;
    }
    if let Some(album) = media.album {
      track.album = album.into();
    }
    if let Some(artist) = media.artist {
      track.artist = artist.clone().into();
      track.artists = vec![artist.into()];
    }
    track.image_id = media.artwork_id.unwrap_or_default();
    if let Some(duration) = media.duration_ms {
      track.duration_ms = duration;
    }
    if let Some(liked) = media.liked {
      track.saved = liked;
    }
    self.track = Some(track);

    let mut accept_position = true;
    if let Some(playing) = playback.playing {
      match self.active_transport_intent() {
        Some(expected) if playing == expected => {
          self.playing = playing;
          self.transport_intent = None;
        }
        Some(_) => {
          accept_position = false;
        }
        None => {
          self.playing = playing;
        }
      }
    } else if self.active_transport_intent().is_some() {
      accept_position = false;
    }
    if accept_position && let Some(position) = playback.position_ms {
      if self.seek_intent_active() {
        // Stale pre-seek position from iOS; hold the optimistic target
        // until the window closes or iOS catches up on the next bump.
      } else {
        self.seek_intent = None;
        self.position_ms = position as usize;
        self.position_anchor = Some(Instant::now());
      }
    }
    if let Some(shuffle) = playback.shuffle {
      self.options.shuffle = shuffle;
    }
    if let Some(repeat) = playback.repeat {
      self.options.repeat = repeat;
    }
    if let Some(name) = playback.app_display_name {
      self.context_title = name;
    }
    if let Some(bundle) = playback.app_bundle {
      self.context_id = Some(bundle);
    }
  }

  pub fn iap2_shuffle(&self) -> Option<bool> {
    self.iap2_playback.shuffle
  }

  pub fn iap2_repeat_mode(&self) -> Option<RepeatMode> {
    self.iap2_playback.repeat
  }

  pub fn iap2_set_elapsed_time_available(&self) -> Option<bool> {
    self.iap2_playback.set_elapsed_time_available
  }

  pub fn replies(&self) -> (PlayerStateReply, PlayerQueueReply) {
    let merged_meta = self.merged_metadata();
    let merged_play = self.merged_playback();
    let overlay = self.resolve_overlay(&merged_meta);
    let head_art = self.gate_head_art(&merged_meta, &overlay);
    let merged_queue = self.merged_queue(&overlay);
    let effective = self.effective_track();

    let media_item = effective.map(|t| build_media_item(t, &merged_meta, &overlay, head_art.clone()));
    let playback = Playback {
      state: if self.playing {
        PlaybackState::Playing
      } else {
        PlaybackState::Paused
      },
      position_ms: u32::try_from(self.current_position_ms()).unwrap_or(u32::MAX),
      shuffle: self.options.shuffle,
      shuffle_mode: merged_play.shuffle_mode,
      repeat: self.options.repeat,
      queue_index: merged_play.queue_index,
      queue_count: merged_play.queue_count,
      queue_chapter_index: merged_play.queue_chapter_index,
      set_elapsed_time_available: merged_play.set_elapsed_time_available,
      queue_list_avail: merged_play.queue_list_avail,
      apple_music_radio_ad: merged_play.apple_music_radio_ad,
    };
    let options = PlayerOptions {
      speed: merged_play.playback_speed.unwrap_or(self.playback_speed as f32),
      crossfade_ms: None,
    };
    let queue_current = effective.map(|t| build_queue_item(t, &merged_meta, &overlay, head_art.clone()));

    let state = PlayerStateReply {
      state: WirePlayerState {
        track: media_item,
        playback,
        queue: merged_queue.clone(),
        options,
      },
    };

    let queue = PlayerQueueReply {
      current: queue_current,
      items: merged_queue,
    };

    (state, queue)
  }

  pub fn current_artwork_id(&self) -> Option<String> {
    self.effective_track()?;
    let merged = self.merged_metadata();
    let overlay = self.resolve_overlay(&merged);
    self.gate_head_art(&merged, &overlay)
  }

  fn effective_track(&self) -> Option<&Track> {
    let track = self.track.as_ref()?;
    if track.id.ends_with("0000000000000000") && track.name.is_empty() {
      return None;
    }
    Some(track)
  }
}

fn build_queue_item(track: &Track, merged: &MediaItemUpdate, overlay: &Overlay, art_id: Option<String>) -> QueueItem {
  let uri = match overlay.tier {
    Tier::Exact => overlay.uri.clone().unwrap_or_else(|| track.id.clone()),
    _ => track.id.clone(),
  };
  QueueItem {
    uri,
    title: merged.title.clone(),
    artist: merged.artist.clone(),
    album: merged.album.clone(),
    artwork_id: art_id,
    duration_ms: merged.duration_ms.or(overlay.duration_ms),
    persistent_id: Some(track.id.clone()),
  }
}

fn build_media_item(track: &Track, merged: &MediaItemUpdate, overlay: &Overlay, art_id: Option<String>) -> MediaItem {
  let uri = match overlay.tier {
    Tier::Exact => Some(overlay.uri.clone().unwrap_or_else(|| track.id.clone())),
    _ => Some(track.id.clone()),
  };
  let is_like_supported = if overlay.like_supported {
    Some(true)
  } else {
    merged.is_like_supported
  };
  MediaItem {
    uri,
    persistent_id: Some(track.id.clone()),
    title: merged.title.clone(),
    album: merged.album.clone(),
    album_artist: merged.album_artist.clone(),
    artist: merged.artist.clone(),
    liked: merged.liked,
    artwork_id: art_id,
    duration_ms: merged.duration_ms.or(overlay.duration_ms),
    media_types: merged.media_types.clone(),
    track_number: merged.track_number,
    track_count: merged.track_count,
    is_like_supported,
    is_ban_supported: merged.is_ban_supported,
    is_banned: merged.is_banned,
    chapter_count: merged.chapter_count,
  }
}

fn accumulate_media(target: &mut MediaItemUpdate, src: MediaItemUpdate) {
  if src.persistent_id.is_some() {
    target.persistent_id = src.persistent_id;
  }
  if src.title.is_some() {
    target.title = src.title;
  }
  if src.album.is_some() {
    target.album = src.album;
  }
  if src.album_artist.is_some() {
    target.album_artist = src.album_artist;
  }
  if src.artist.is_some() {
    target.artist = src.artist;
  }
  if src.liked.is_some() {
    target.liked = src.liked;
  }
  if src.artwork_id.is_some() {
    target.artwork_id = src.artwork_id;
  }
  if src.duration_ms.is_some() {
    target.duration_ms = src.duration_ms;
  }
  if src.media_types.is_some() {
    target.media_types = src.media_types;
  }
  if src.track_number.is_some() {
    target.track_number = src.track_number;
  }
  if src.track_count.is_some() {
    target.track_count = src.track_count;
  }
  if src.is_like_supported.is_some() {
    target.is_like_supported = src.is_like_supported;
  }
  if src.is_ban_supported.is_some() {
    target.is_ban_supported = src.is_ban_supported;
  }
  if src.is_banned.is_some() {
    target.is_banned = src.is_banned;
  }
  if src.is_resident_on_device.is_some() {
    target.is_resident_on_device = src.is_resident_on_device;
  }
  if src.chapter_count.is_some() {
    target.chapter_count = src.chapter_count;
  }
}

fn accumulate_playback(target: &mut PlaybackUpdate, src: PlaybackUpdate) {
  if src.playing.is_some() {
    target.playing = src.playing;
  }
  if src.position_ms.is_some() {
    target.position_ms = src.position_ms;
  }
  if src.shuffle.is_some() {
    target.shuffle = src.shuffle;
  }
  if src.shuffle_mode.is_some() {
    target.shuffle_mode = src.shuffle_mode;
  }
  if src.repeat.is_some() {
    target.repeat = src.repeat;
  }
  if src.app_bundle.is_some() {
    target.app_bundle = src.app_bundle;
  }
  if src.app_display_name.is_some() {
    target.app_display_name = src.app_display_name;
  }
  if src.queue_index.is_some() {
    target.queue_index = src.queue_index;
  }
  if src.queue_count.is_some() {
    target.queue_count = src.queue_count;
  }
  if src.queue_chapter_index.is_some() {
    target.queue_chapter_index = src.queue_chapter_index;
  }
  if src.playback_speed.is_some() {
    target.playback_speed = src.playback_speed;
  }
  if src.set_elapsed_time_available.is_some() {
    target.set_elapsed_time_available = src.set_elapsed_time_available;
  }
  if src.queue_list_avail.is_some() {
    target.queue_list_avail = src.queue_list_avail;
  }
  if src.apple_music_radio_ad.is_some() {
    target.apple_music_radio_ad = src.apple_music_radio_ad;
  }
  if src.apple_music_radio_station_name.is_some() {
    target.apple_music_radio_station_name = src.apple_music_radio_station_name;
  }
}

#[derive(Debug, Clone)]
struct EnrichedEntry {
  uri: String,
  artwork_id: Option<String>,
  norm_title: String,
  norm_artist: String,
  duration_ms: Option<u32>,
}

impl EnrichedEntry {
  fn from_head(head: &QueueItem) -> Self {
    EnrichedEntry {
      uri: head.uri.clone(),
      artwork_id: head.artwork_id.clone(),
      norm_title: head.title.as_deref().map(normalize).unwrap_or_default(),
      norm_artist: head.artist.as_deref().map(artist_primary).unwrap_or_default(),
      duration_ms: head.duration_ms,
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tier {
  Bare,
  Content,
  Exact,
}

struct Overlay {
  tier: Tier,
  art: Option<String>,
  uri: Option<String>,
  duration_ms: Option<u32>,
  like_supported: bool,
  queue: Option<Vec<QueueItem>>,
}

impl Overlay {
  fn bare() -> Self {
    Overlay {
      tier: Tier::Bare,
      art: None,
      uri: None,
      duration_ms: None,
      like_supported: false,
      queue: None,
    }
  }
}

struct Cand {
  norm_title: String,
  norm_artist: String,
  duration_ms: Option<u32>,
  artwork_id: Option<String>,
}

impl Cand {
  fn from_queue(item: &QueueItem) -> Self {
    Cand {
      norm_title: item.title.as_deref().map(normalize).unwrap_or_default(),
      norm_artist: item.artist.as_deref().map(artist_primary).unwrap_or_default(),
      duration_ms: item.duration_ms,
      artwork_id: item.artwork_id.clone(),
    }
  }

  fn from_entry(entry: &EnrichedEntry) -> Self {
    Cand {
      norm_title: entry.norm_title.clone(),
      norm_artist: entry.norm_artist.clone(),
      duration_ms: entry.duration_ms,
      artwork_id: entry.artwork_id.clone(),
    }
  }
}

fn content_agrees(entry: &EnrichedEntry, id: &MediaItemUpdate) -> bool {
  let Some(title) = id.title.as_deref() else {
    return false;
  };
  if normalize(title) != entry.norm_title || entry.norm_title.is_empty() {
    return false;
  }
  let artist = id.artist.as_deref().map(artist_primary).unwrap_or_default();
  if !artist_overlap(&artist, &entry.norm_artist) {
    return false;
  }
  duration_ok(entry.duration_ms, id.duration_ms)
}

fn best_content_match(id: &MediaItemUpdate, cands: impl Iterator<Item = Cand>) -> Option<Cand> {
  let title = id.title.as_deref().map(normalize)?;
  if title.is_empty() {
    return None;
  }
  let artist = id.artist.as_deref().map(artist_primary).unwrap_or_default();
  let mut best: Option<Cand> = None;
  for cand in cands {
    if cand.artwork_id.is_none() || cand.norm_title != title {
      continue;
    }
    if !artist_overlap(&artist, &cand.norm_artist) || !duration_ok(cand.duration_ms, id.duration_ms) {
      continue;
    }
    let confirmed = both_present(cand.duration_ms, id.duration_ms);
    match &best {
      None => best = Some(cand),
      Some(b) if confirmed && !both_present(b.duration_ms, id.duration_ms) => best = Some(cand),
      _ => {}
    }
  }
  best
}

fn both_present(a: Option<u32>, b: Option<u32>) -> bool {
  matches!((a, b), (Some(_), Some(_)))
}

fn duration_ok(a: Option<u32>, b: Option<u32>) -> bool {
  match (a, b) {
    (Some(x), Some(y)) => x.abs_diff(y) <= DURATION_TOLERANCE_MS,
    _ => true,
  }
}

fn artist_overlap(a: &str, b: &str) -> bool {
  !a.is_empty() && !b.is_empty() && (a == b || a.contains(b) || b.contains(a))
}

fn artist_primary(s: &str) -> String {
  let lower = s.to_lowercase();
  let first = lower.split([',', '&']).next().unwrap_or(&lower);
  let first = first.split(" feat").next().unwrap_or(first);
  normalize(first)
}

fn normalize(s: &str) -> String {
  let lower = s.to_lowercase();
  let head = lower.split(" - ").next().unwrap_or(&lower);
  let mut out = String::with_capacity(head.len());
  let mut depth: i32 = 0;
  let mut pending_space = false;
  for ch in head.chars() {
    match ch {
      '(' | '[' | '{' => depth += 1,
      ')' | ']' | '}' => depth = (depth - 1).max(0),
      _ if depth > 0 => {}
      c if c.is_alphanumeric() => {
        if pending_space && !out.is_empty() {
          out.push(' ');
        }
        pending_space = false;
        out.push(c);
      }
      _ => pending_space = true,
    }
  }
  out
}

#[cfg(test)]
mod tests {
  use libbridgething::CompanionAuthorityScope;

  use super::*;

  fn iap2_track(persistent_id: &str, title: &str) -> NowPlayingUpdate {
    NowPlayingUpdate {
      media_item: Some(MediaItemUpdate {
        persistent_id: Some(persistent_id.to_string()),
        title: Some(title.to_string()),
        ..MediaItemUpdate::default()
      }),
      playback: None,
    }
  }

  fn companion_track(persistent_id: &str, title: &str, artwork_id: Option<&str>) -> NowPlayingUpdate {
    NowPlayingUpdate {
      media_item: Some(MediaItemUpdate {
        persistent_id: Some(persistent_id.to_string()),
        title: Some(title.to_string()),
        artwork_id: artwork_id.map(|s| s.to_string()),
        ..MediaItemUpdate::default()
      }),
      playback: None,
    }
  }

  fn artwork_id_of(state: &PlayerState) -> Option<String> {
    state.replies().0.state.track.and_then(|t| t.artwork_id)
  }

  #[test]
  fn iap2_now_playing_does_not_emit_artwork_id_until_apply_artwork_id() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_now_playing(NowPlayingSource::Iap2, iap2_track("iap2:track:abc", "Heart of Glass"));
    assert_eq!(artwork_id_of(&state), None);

    state.apply_artwork_id(NowPlayingSource::Iap2, "iap2/art/abc/5".to_string());
    assert_eq!(artwork_id_of(&state), Some("iap2/art/abc/5".to_string()));
  }

  #[test]
  fn idle_persistent_id_zero_suppresses_track_emission() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_now_playing(
      NowPlayingSource::Iap2,
      NowPlayingUpdate {
        media_item: Some(MediaItemUpdate {
          persistent_id: Some("iap2:track:0000000000000000".to_string()),
          title: Some(String::new()),
          ..MediaItemUpdate::default()
        }),
        playback: None,
      },
    );
    assert!(state.replies().0.state.track.is_none());
    assert_eq!(state.current_artwork_id(), None);
  }

  #[test]
  fn pid_zero_with_real_title_emits_track() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_now_playing(
      NowPlayingSource::Iap2,
      iap2_track(
        "iap2:track:0000000000000000",
        "99.9% Of Elden Ring Players CAN'T Beat This Mod",
      ),
    );
    let track = state.replies().0.state.track.expect("track present");
    assert_eq!(
      track.title.as_deref(),
      Some("99.9% Of Elden Ring Players CAN'T Beat This Mod")
    );
    assert_eq!(track.persistent_id.as_deref(), Some("iap2:track:0000000000000000"));
  }

  #[test]
  fn track_change_clears_stale_iap2_art() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_now_playing(NowPlayingSource::Iap2, iap2_track("iap2:track:a", "Track A"));
    state.apply_artwork_id(NowPlayingSource::Iap2, "iap2/art/a/5".to_string());
    assert_eq!(artwork_id_of(&state), Some("iap2/art/a/5".to_string()));

    state.apply_now_playing(NowPlayingSource::Iap2, iap2_track("iap2:track:b", "Track B"));
    assert_eq!(artwork_id_of(&state), None);
  }

  #[test]
  fn companion_authoritative_clears_iap2_art_on_wire() {
    let auth = AuthorityRegistry::new();
    let mut state = PlayerState::new(auth.clone());
    state.apply_now_playing(NowPlayingSource::Iap2, iap2_track("track:a", "A"));
    state.apply_artwork_id(NowPlayingSource::Iap2, "iap2/art/a/5".to_string());
    assert_eq!(artwork_id_of(&state), Some("iap2/art/a/5".to_string()));

    auth.claim(CompanionAuthorityScope::NowPlayingMetadata);
    state.apply_now_playing(NowPlayingSource::Companion, companion_track("track:a", "A", None));
    assert_eq!(
      artwork_id_of(&state),
      None,
      "companion authoritative without artwork_id must NOT leak iap2 art onto the wire"
    );
  }

  #[test]
  fn companion_release_restores_iap2_art() {
    let auth = AuthorityRegistry::new();
    let mut state = PlayerState::new(auth.clone());
    state.apply_now_playing(NowPlayingSource::Iap2, iap2_track("track:a", "A"));
    state.apply_artwork_id(NowPlayingSource::Iap2, "iap2/art/a/5".to_string());

    auth.claim(CompanionAuthorityScope::NowPlayingMetadata);
    state.apply_now_playing(
      NowPlayingSource::Companion,
      companion_track("track:a", "A", Some("spotify/track/a/image")),
    );
    assert_eq!(artwork_id_of(&state), Some("spotify/track/a/image".to_string()));

    auth.release(CompanionAuthorityScope::NowPlayingMetadata);
    state.apply_now_playing(NowPlayingSource::Iap2, iap2_track("track:a", "A"));
    assert_eq!(artwork_id_of(&state), Some("iap2/art/a/5".to_string()));
  }

  #[test]
  fn current_artwork_id_filters_idle_track() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_now_playing(
      NowPlayingSource::Iap2,
      NowPlayingUpdate {
        media_item: Some(MediaItemUpdate {
          persistent_id: Some("iap2:track:0000000000000000".to_string()),
          title: Some(String::new()),
          ..MediaItemUpdate::default()
        }),
        playback: None,
      },
    );
    state.apply_artwork_id(NowPlayingSource::Iap2, "iap2/art/0000000000000000/1".to_string());
    assert_eq!(state.current_artwork_id(), None);
  }

  #[test]
  fn build_media_item_emits_none_for_empty_image_id() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_now_playing(NowPlayingSource::Iap2, iap2_track("track:a", "A"));
    let track = state.replies().0.state.track.expect("track present");
    assert_eq!(track.artwork_id, None);
  }

  fn iap2_full(pid: &str, title: &str, artist: &str, duration_ms: Option<u32>) -> NowPlayingUpdate {
    NowPlayingUpdate {
      media_item: Some(MediaItemUpdate {
        persistent_id: Some(pid.to_string()),
        title: Some(title.to_string()),
        artist: Some(artist.to_string()),
        duration_ms,
        ..MediaItemUpdate::default()
      }),
      playback: None,
    }
  }

  fn qitem(uri: &str, title: &str, artist: &str, art: Option<&str>, duration_ms: Option<u32>) -> QueueItem {
    QueueItem {
      uri: uri.to_string(),
      title: Some(title.to_string()),
      artist: Some(artist.to_string()),
      album: None,
      artwork_id: art.map(str::to_string),
      duration_ms,
      persistent_id: None,
    }
  }

  fn offer(anchor: &str, head: Option<QueueItem>) -> NowPlayingEnrichment {
    NowPlayingEnrichment {
      anchor_pid: Some(anchor.to_string()),
      head,
      context: None,
    }
  }

  fn qsnap(items: Vec<QueueItem>) -> QueueSnapshot {
    QueueSnapshot {
      order: items.iter().map(|i| i.uri.clone()).collect(),
      items,
    }
  }

  fn media(state: &PlayerState) -> MediaItem {
    state.replies().0.state.track.expect("track present")
  }

  #[test]
  fn no_offer_is_bare() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_now_playing(
      NowPlayingSource::Iap2,
      iap2_full("iap2:track:a", "Song", "Artist", Some(180000)),
    );
    let m = media(&state);
    assert_eq!(m.uri.as_deref(), Some("iap2:track:a"));
    assert_eq!(m.artwork_id, None);
    assert_eq!(m.is_like_supported, None);
  }

  #[test]
  fn exact_match_overlays_uri_art_heart_and_queue() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_now_playing(
      NowPlayingSource::Iap2,
      iap2_full("iap2:track:a", "Song", "Artist", Some(180000)),
    );
    let up_next = qitem("spotify:track:2", "Next", "Other", Some("spotify/img/2"), Some(200000));
    state.apply_enrichment(offer(
      "iap2:track:a",
      Some(qitem(
        "spotify:track:1",
        "Song",
        "Artist",
        Some("spotify/img/1"),
        Some(180000),
      )),
    ));
    state.apply_companion_queue(qsnap(vec![up_next.clone()]));

    let m = media(&state);
    assert_eq!(m.uri.as_deref(), Some("spotify:track:1"));
    assert_eq!(m.persistent_id.as_deref(), Some("iap2:track:a"));
    assert_eq!(m.artwork_id.as_deref(), Some("spotify/img/1"));
    assert_eq!(m.is_like_supported, Some(true));
    assert_eq!(state.replies().1.items, vec![up_next]);
  }

  #[test]
  fn companion_queue_rebuilds_order_from_deduped_items() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    let a = qitem("spotify:track:a", "A", "X", Some("img/a"), Some(1000));
    let b = qitem("spotify:track:b", "B", "X", Some("img/b"), Some(2000));
    let c = qitem("spotify:track:c", "C", "X", Some("img/c"), Some(3000));

    state.apply_companion_queue(qsnap(vec![a.clone(), b.clone(), c.clone()]));
    assert_eq!(state.companion_queue, vec![a, b.clone(), c.clone()]);

    // advance: a drops off the front, d appends. only d carries metadata; b and c
    // are referenced by order alone and reused from the prior queue.
    let d = qitem("spotify:track:d", "D", "X", Some("img/d"), Some(4000));
    state.apply_companion_queue(QueueSnapshot {
      order: vec![
        "spotify:track:b".into(),
        "spotify:track:c".into(),
        "spotify:track:d".into(),
      ],
      items: vec![d.clone()],
    });
    assert_eq!(state.companion_queue, vec![b, c, d]);
  }

  #[test]
  fn empty_companion_queue_falls_back_to_iap2_queue() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_now_playing(
      NowPlayingSource::Iap2,
      iap2_full("iap2:track:a", "Song", "Artist", Some(180000)),
    );
    state.replace_iap2_queue(vec![qitem("iap2:track:next", "Next", "Artist", None, Some(200000))]);
    // the offer makes the head an Exact match with a current anchor, but no QueueChanged has
    // arrived yet: an empty companion_queue must not blank the iAP2 queue.
    state.apply_enrichment(offer(
      "iap2:track:a",
      Some(qitem(
        "spotify:track:a",
        "Song",
        "Artist",
        Some("spotify/img/a"),
        Some(180000),
      )),
    ));
    assert_eq!(
      state.replies().1.items.first().map(|q| q.uri.clone()),
      Some("iap2:track:next".to_string()),
      "empty companion queue falls through to the iAP2 queue"
    );
  }

  #[test]
  fn head_art_gates_iap2_until_spotify_bytes_land() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_now_playing(
      NowPlayingSource::Iap2,
      iap2_full("iap2:track:a", "Song", "Artist", Some(180000)),
    );
    state.apply_artwork_id(NowPlayingSource::Iap2, "iap2/art/aa/1".into());
    state.apply_enrichment(offer(
      "iap2:track:a",
      Some(qitem(
        "spotify:track:1",
        "Song",
        "Artist",
        Some("spotify/img/1"),
        Some(180000),
      )),
    ));

    // only the iAP2 bytes are cached: show the iAP2 art, not the uncached Spotify id.
    state.note_asset_ready("iap2/art/aa/1".into());
    assert_eq!(media(&state).artwork_id.as_deref(), Some("iap2/art/aa/1"));

    // Spotify bytes land: upgrade to the Spotify id.
    state.note_asset_ready("spotify/img/1".into());
    assert_eq!(media(&state).artwork_id.as_deref(), Some("spotify/img/1"));

    // Spotify art evicted: fall back to the still-present iAP2 art.
    state.note_asset_cleared("spotify/img/1");
    assert_eq!(media(&state).artwork_id.as_deref(), Some("iap2/art/aa/1"));
  }

  #[test]
  fn veto_rejects_mismatched_head() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_now_playing(
      NowPlayingSource::Iap2,
      iap2_full("iap2:track:a", "Real Song", "Artist", Some(180000)),
    );
    state.apply_enrichment(offer(
      "iap2:track:a",
      Some(qitem(
        "spotify:track:9",
        "Completely Different",
        "Nobody",
        Some("spotify/img/9"),
        Some(9000),
      )),
    ));

    let m = media(&state);
    assert_eq!(
      m.uri.as_deref(),
      Some("iap2:track:a"),
      "no spotify uri on a veto-rejected head"
    );
    assert_eq!(m.is_like_supported, None, "heart hidden on veto");
    assert_eq!(m.artwork_id, None);
  }

  #[test]
  fn skip_back_restores_from_by_pid_without_fresh_offer() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_now_playing(
      NowPlayingSource::Iap2,
      iap2_full("iap2:track:a", "Song A", "Artist", Some(180000)),
    );
    state.apply_enrichment(offer(
      "iap2:track:a",
      Some(qitem(
        "spotify:track:a",
        "Song A",
        "Artist",
        Some("spotify/img/a"),
        Some(180000),
      )),
    ));
    state.apply_now_playing(
      NowPlayingSource::Iap2,
      iap2_full("iap2:track:b", "Song B", "Artist", Some(200000)),
    );
    state.apply_enrichment(offer(
      "iap2:track:b",
      Some(qitem(
        "spotify:track:b",
        "Song B",
        "Artist",
        Some("spotify/img/b"),
        Some(200000),
      )),
    ));

    state.apply_now_playing(
      NowPlayingSource::Iap2,
      iap2_full("iap2:track:a", "Song A", "Artist", Some(180000)),
    );
    let m = media(&state);
    assert_eq!(m.uri.as_deref(), Some("spotify:track:a"));
    assert_eq!(m.artwork_id.as_deref(), Some("spotify/img/a"));
    assert_eq!(m.is_like_supported, Some(true));
  }

  #[test]
  fn skip_forward_content_match_supplies_art_only() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_now_playing(
      NowPlayingSource::Iap2,
      iap2_full("iap2:track:a", "Song A", "Artist", Some(180000)),
    );
    state.apply_enrichment(offer(
      "iap2:track:a",
      Some(qitem(
        "spotify:track:a",
        "Song A",
        "Artist",
        Some("spotify/img/a"),
        Some(180000),
      )),
    ));
    state.apply_companion_queue(qsnap(vec![qitem(
      "spotify:track:b",
      "Song B",
      "Artist",
      Some("spotify/img/b"),
      Some(200000),
    )]));

    state.apply_now_playing(
      NowPlayingSource::Iap2,
      iap2_full("iap2:track:b", "Song B", "Artist", Some(200000)),
    );
    let m = media(&state);
    assert_eq!(
      m.artwork_id.as_deref(),
      Some("spotify/img/b"),
      "predictive queue art fills instantly"
    );
    assert_eq!(m.uri.as_deref(), Some("iap2:track:b"), "no uri on a content match");
    assert_eq!(m.is_like_supported, None, "heart hidden on a content match");
  }

  #[test]
  fn content_match_rejected_on_duration_mismatch() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_now_playing(
      NowPlayingSource::Iap2,
      iap2_full("iap2:track:a", "Song A", "Artist", Some(180000)),
    );
    state.apply_enrichment(offer(
      "iap2:track:a",
      Some(qitem(
        "spotify:track:a",
        "Song A",
        "Artist",
        Some("spotify/img/a"),
        Some(180000),
      )),
    ));
    state.apply_companion_queue(qsnap(vec![qitem(
      "spotify:track:b",
      "Live Take",
      "Artist",
      Some("spotify/img/b"),
      Some(100000),
    )]));

    state.apply_now_playing(
      NowPlayingSource::Iap2,
      iap2_full("iap2:track:b", "Live Take", "Artist", Some(400000)),
    );
    let m = media(&state);
    assert_eq!(
      m.artwork_id, None,
      "duration delta beyond tolerance rejects the art match"
    );
  }

  #[test]
  fn late_iap2_art_does_not_downgrade_spotify_art() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_now_playing(
      NowPlayingSource::Iap2,
      iap2_full("iap2:track:a", "Song", "Artist", Some(180000)),
    );
    state.apply_enrichment(offer(
      "iap2:track:a",
      Some(qitem(
        "spotify:track:a",
        "Song",
        "Artist",
        Some("spotify/img/a"),
        Some(180000),
      )),
    ));
    assert_eq!(media(&state).artwork_id.as_deref(), Some("spotify/img/a"));

    state.apply_artwork_id(NowPlayingSource::Iap2, "iap2/art/a/3".to_string());
    assert_eq!(
      media(&state).artwork_id.as_deref(),
      Some("spotify/img/a"),
      "a matched spotify art is not replaced by a late iAP2 art"
    );
  }

  #[test]
  fn normalize_folds_remaster_and_feat_suffixes() {
    assert_eq!(normalize("Song (feat. Someone) - Remastered 2011"), normalize("song"));
    assert_eq!(normalize("Heart of Glass [Live]"), normalize("heart of glass"));
  }

  #[test]
  fn skip_back_with_unstable_pid_recovers_art_then_heart() {
    // the bug: iAP2 gives a real pid on skip-forward but a pid-less ("nonmusic") identity for the
    // SAME track on skip-back. the retained forward entry is a content candidate, so the back
    // track's spotify art loads instantly by title (not the forward track's art, no heart yet), and
    // the companion's fresh offer (anchored to the nonmusic key) then lights the heart.
    let mut state = PlayerState::new(AuthorityRegistry::new());

    state.apply_now_playing(
      NowPlayingSource::Iap2,
      iap2_full("iap2:track:aaaa", "Song A", "X", Some(180_000)),
    );
    state.apply_enrichment(offer(
      "iap2:track:aaaa",
      Some(qitem(
        "spotify:track:a",
        "Song A",
        "X",
        Some("spotify/img/a"),
        Some(180_000),
      )),
    ));
    assert_eq!(media(&state).artwork_id.as_deref(), Some("spotify/img/a"));

    state.apply_now_playing(
      NowPlayingSource::Iap2,
      iap2_full("iap2:track:bbbb", "Song B", "X", Some(200_000)),
    );
    state.apply_enrichment(offer(
      "iap2:track:bbbb",
      Some(qitem(
        "spotify:track:b",
        "Song B",
        "X",
        Some("spotify/img/b"),
        Some(200_000),
      )),
    ));
    assert_eq!(media(&state).artwork_id.as_deref(), Some("spotify/img/b"));

    // skip back to A, but iAP2 reports it pid-less -> a synthesized nonmusic identity key.
    state.apply_now_playing(
      NowPlayingSource::Iap2,
      iap2_full("iap2:track:nonmusic_songa", "Song A", "X", Some(180_000)),
    );
    let m = media(&state);
    assert_eq!(
      m.artwork_id.as_deref(),
      Some("spotify/img/a"),
      "retained forward entry supplies A's art by content match despite the key change"
    );
    assert_eq!(
      m.uri.as_deref(),
      Some("iap2:track:nonmusic_songa"),
      "no uri/heart on a content match"
    );
    assert_eq!(m.is_like_supported, None);

    state.apply_enrichment(offer(
      "iap2:track:nonmusic_songa",
      Some(qitem(
        "spotify:track:a",
        "Song A",
        "X",
        Some("spotify/img/a"),
        Some(180_000),
      )),
    ));
    let m = media(&state);
    assert_eq!(
      m.uri.as_deref(),
      Some("spotify:track:a"),
      "fresh offer resolves the uri"
    );
    assert_eq!(m.is_like_supported, Some(true), "heart lit after the fresh offer");
    assert_eq!(m.artwork_id.as_deref(), Some("spotify/img/a"));
  }

  #[test]
  fn unstable_pid_back_track_never_shows_the_forward_arts() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_now_playing(
      NowPlayingSource::Iap2,
      iap2_full("iap2:track:bbbb", "Song B", "X", Some(200_000)),
    );
    state.apply_enrichment(offer(
      "iap2:track:bbbb",
      Some(qitem(
        "spotify:track:b",
        "Song B",
        "X",
        Some("spotify/img/b"),
        Some(200_000),
      )),
    ));
    state.apply_companion_queue(qsnap(vec![qitem(
      "spotify:track:c",
      "Song C",
      "X",
      Some("spotify/img/c"),
      Some(210_000),
    )]));
    assert_eq!(media(&state).artwork_id.as_deref(), Some("spotify/img/b"));

    state.apply_now_playing(
      NowPlayingSource::Iap2,
      iap2_full("iap2:track:nonmusic_unknown", "Some Unseen Track", "Y", Some(123_000)),
    );
    assert_eq!(
      media(&state).artwork_id,
      None,
      "no match -> bare, never the forward/queue art"
    );
  }
}
