use std::{
  collections::{HashMap, HashSet},
  time::{Duration, Instant},
};

use libbridgething::{
  CompanionAuthorityScope, CurrentlyActiveApplication, MediaItem, MediaItemUpdate, NowPlayingUpdate, Playback,
  PlaybackContext, PlaybackOptions, PlaybackState, PlaybackUpdate, PlayerOptions, PlayerState as WirePlayerState,
  QueueItem, RepeatMode, Track,
  client::{PlayerQueueReply, PlayerStateReply},
  gateway::QueueSnapshot,
};

use super::is_synthetic_uri;
use crate::authority::AuthorityRegistry;

const TRANSPORT_INTENT_WINDOW: Duration = Duration::from_millis(1500);
const SEEK_INTENT_WINDOW: Duration = Duration::from_millis(1500);
const POSITION_RESYNC_TOLERANCE_MS: usize = 2000;
const RECENTLY_PLAYED_CAP: usize = 25;
const RECENTLY_PLAYED_MIN_MS: usize = 30_000;

#[derive(Debug, Clone)]
pub struct PlayerState {
  authority: AuthorityRegistry,

  pub playing: bool,

  context: Option<PlaybackContext>,

  position_anchor: Option<Instant>,
  pub position_ms: usize,
  pub playback_speed: f64,

  pub track: Option<Track>,

  pub options: PlaybackOptions,

  iap2_metadata: MediaItemUpdate,
  iap2_playback: PlaybackUpdate,
  companion_metadata: MediaItemUpdate,
  companion_playback: PlaybackUpdate,

  companion_queue: Vec<QueueItem>,
  recently_played: Vec<QueueItem>,
  recently_played_gen: u64,

  present_ids: HashSet<String>,

  transport_intent: Option<TransportIntent>,
  seek_intent: Option<SeekIntent>,

  position_resync: bool,
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

      context: None,

      position_ms: 0,
      position_anchor: None,
      playback_speed: 1.0,

      track: None,

      options: PlaybackOptions::default(),

      iap2_metadata: MediaItemUpdate::default(),
      iap2_playback: PlaybackUpdate::default(),
      companion_metadata: MediaItemUpdate::default(),
      companion_playback: PlaybackUpdate::default(),

      companion_queue: Vec::new(),
      recently_played: Vec::new(),
      recently_played_gen: 0,

      present_ids: HashSet::new(),

      transport_intent: None,
      seek_intent: None,

      position_resync: false,
    }
  }

  pub(crate) fn take_position_resync(&mut self) -> bool {
    std::mem::take(&mut self.position_resync)
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

  fn companion_now_playing_authoritative(&self, scope: CompanionAuthorityScope) -> bool {
    self.authority.is_authoritative(scope) && !self.iap2_foreground_is_other_app()
  }

  fn iap2_foreground_is_other_app(&self) -> bool {
    let Some(iap2) = self.iap2_playback.app_bundle.as_deref() else {
      return false;
    };
    match self.authority.companion_app_bundle() {
      Some(comp) => !comp.eq_ignore_ascii_case(iap2),
      None => false,
    }
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

  pub(crate) fn current_position_ms(&self) -> usize {
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

  pub(crate) fn apply_companion_queue(&mut self, snapshot: QueueSnapshot) {
    let QueueSnapshot { order, items } = snapshot;
    let by_uri: HashMap<&str, &QueueItem> = items.iter().map(|q| (q.uri.as_str(), q)).collect();
    let mut rebuilt = Vec::with_capacity(order.len());
    for uri in &order {
      if let Some(item) = by_uri.get(uri.as_str()) {
        rebuilt.push((*item).clone());
      }
    }
    if rebuilt.len() != order.len() {
      tracing::warn!(
        ordered = order.len(),
        resolved = rebuilt.len(),
        "companion queue: ordered uris without an item in the snapshot were dropped"
      );
    }
    self.companion_queue = rebuilt;
  }

  pub(crate) fn note_rolled_off(&mut self, outgoing: QueueItem, played_ms: usize) {
    if self.recently_played.first().map(|q| &q.uri) != Some(&outgoing.uri) {
      self.recently_played.insert(0, outgoing);
      self.recently_played.truncate(RECENTLY_PLAYED_CAP);
    }
    if played_ms >= RECENTLY_PLAYED_MIN_MS {
      self.recently_played_gen = self.recently_played_gen.wrapping_add(1);
    }
  }

  pub(crate) fn recently_played_gen(&self) -> u64 {
    self.recently_played_gen
  }

  pub(crate) fn reset_companion(&mut self) {
    self.companion_queue.clear();
    self.recently_played.clear();
    self.recently_played_gen = self.recently_played_gen.wrapping_add(1);
  }

  pub(crate) fn apply_companion_snapshot(&mut self, snapshot: WirePlayerState) {
    let WirePlayerState {
      track,
      playback,
      queue: _,
      options,
      context,
    } = snapshot;

    self.context = context;

    self.companion_metadata = match track {
      Some(t) => MediaItemUpdate {
        persistent_id: t.persistent_id,
        title: t.title,
        album: t.album,
        album_uri: t.album_uri,
        album_artist: t.album_artist,
        artist: t.artist,
        artist_uri: t.artist_uri,
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

    let merged_meta = self.merged_metadata();
    let merged_play = self.merged_playback();
    self.apply_merged(merged_meta, merged_play);
  }

  pub(crate) fn apply_now_playing(&mut self, update: NowPlayingUpdate) {
    let NowPlayingUpdate { media_item, playback } = update;

    let meta_target = &mut self.iap2_metadata;
    let play_target = &mut self.iap2_playback;

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

  pub(crate) fn apply_artwork_id(&mut self, asset_id: String) {
    self.iap2_metadata.artwork_id = Some(asset_id);

    let merged_meta = self.merged_metadata();
    let merged_play = self.merged_playback();
    self.apply_merged(merged_meta, merged_play);
  }

  fn merged_metadata(&self) -> MediaItemUpdate {
    let companion_authoritative = self.companion_now_playing_authoritative(CompanionAuthorityScope::NowPlayingMetadata);
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
        album_uri: self
          .companion_metadata
          .album_uri
          .clone()
          .or_else(|| self.iap2_metadata.album_uri.clone()),
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
        artist_uri: self
          .companion_metadata
          .artist_uri
          .clone()
          .or_else(|| self.iap2_metadata.artist_uri.clone()),
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
    let companion_authoritative = self.companion_now_playing_authoritative(CompanionAuthorityScope::NowPlayingPlayback);
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
        // Stale pre-seek position from iOS
      } else {
        let position = position as usize;
        let current = self.current_position_ms();
        let duration_known = self.track.as_ref().is_some_and(|t| t.duration_ms > 0);
        let riding = same_track
          && self.playing
          && duration_known
          && self.position_anchor.is_some()
          && position.abs_diff(current) <= POSITION_RESYNC_TOLERANCE_MS;

        if riding && position < current {
          // stale backward tick; ignore
        } else {
          if !riding {
            self.position_resync = true;
          }
          self.seek_intent = None;
          self.position_ms = position;
          self.position_anchor = Some(Instant::now());
        }
      }
    }
    if let Some(shuffle) = playback.shuffle {
      self.options.shuffle = shuffle;
    }
    if let Some(repeat) = playback.repeat {
      self.options.repeat = repeat;
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
    let companion_authoritative = self.companion_now_playing_authoritative(CompanionAuthorityScope::NowPlayingMetadata);
    let head_art = merged_meta.artwork_id.clone().filter(|s| !s.is_empty());
    let raw_upcoming = if companion_authoritative {
      self.companion_queue.clone()
    } else {
      Vec::new()
    };
    let effective = self.effective_track();
    let context = effective.and_then(|_| companion_authoritative.then(|| self.context.clone()).flatten());

    let queue_current = effective.map(|t| build_queue_item(t, &merged_meta, head_art.clone()));
    let next = derive_next(raw_upcoming, queue_current.as_ref());

    let media_item = effective.map(|t| build_media_item(t, &merged_meta, head_art.clone()));
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

    let state = PlayerStateReply {
      active_app: self.active_app(),
      state: WirePlayerState {
        track: media_item,
        playback,
        queue: next.clone(),
        options,
        context,
      },
    };

    let queue = PlayerQueueReply {
      current: queue_current,
      items: next,
      previous: self.recently_played.clone(),
    };

    (state, queue)
  }

  pub fn current_artwork_id(&self) -> Option<String> {
    self.effective_track()?;
    self.merged_metadata().artwork_id.filter(|s| !s.is_empty())
  }

  pub fn active_app(&self) -> Option<CurrentlyActiveApplication> {
    let track = self.effective_track()?;
    if !is_synthetic_uri(&track.id) {
      return None;
    }
    let play = self.merged_playback();
    let bundle = play.app_bundle.clone();
    let name = play
      .app_display_name
      .clone()
      .or_else(|| bundle.clone())
      .unwrap_or_else(|| "phone".to_string());
    Some(CurrentlyActiveApplication {
      id: bundle.unwrap_or_default(),
      name,
    })
  }

  fn effective_track(&self) -> Option<&Track> {
    let track = self.track.as_ref()?;
    if track.id.ends_with("0000000000000000") && track.name.is_empty() {
      return None;
    }
    Some(track)
  }
}

fn derive_next(upcoming: Vec<QueueItem>, current: Option<&QueueItem>) -> Vec<QueueItem> {
  let Some(cur) = current else {
    return upcoming;
  };
  match upcoming.iter().position(|q| queue_items_match(q, cur)) {
    Some(pos) => upcoming[pos + 1..].to_vec(),
    None => upcoming,
  }
}

fn queue_items_match(a: &QueueItem, b: &QueueItem) -> bool {
  (!a.uri.is_empty() && a.uri == b.uri) || (a.persistent_id.is_some() && a.persistent_id == b.persistent_id)
}

fn build_queue_item(track: &Track, merged: &MediaItemUpdate, art_id: Option<String>) -> QueueItem {
  QueueItem {
    uri: track.id.clone(),
    title: merged.title.clone(),
    artist: merged.artist.clone(),
    artist_uri: merged.artist_uri.clone(),
    album: merged.album.clone(),
    album_uri: merged.album_uri.clone(),
    artwork_id: art_id,
    duration_ms: merged.duration_ms,
    persistent_id: Some(track.id.clone()),
  }
}

fn build_media_item(track: &Track, merged: &MediaItemUpdate, art_id: Option<String>) -> MediaItem {
  let uri = if is_synthetic_uri(&track.id) {
    None
  } else {
    Some(track.id.clone())
  };
  MediaItem {
    uri,
    persistent_id: Some(track.id.clone()),
    title: merged.title.clone(),
    album: merged.album.clone(),
    album_uri: merged.album_uri.clone(),
    album_artist: merged.album_artist.clone(),
    artist: merged.artist.clone(),
    artist_uri: merged.artist_uri.clone(),
    liked: merged.liked,
    artwork_id: art_id,
    duration_ms: merged.duration_ms,
    media_types: merged.media_types.clone(),
    track_number: merged.track_number,
    track_count: merged.track_count,
    is_like_supported: merged.is_like_supported,
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
  if src.album_uri.is_some() {
    target.album_uri = src.album_uri;
  }
  if src.album_artist.is_some() {
    target.album_artist = src.album_artist;
  }
  if src.artist.is_some() {
    target.artist = src.artist;
  }
  if src.artist_uri.is_some() {
    target.artist_uri = src.artist_uri;
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

  fn companion_snapshot(persistent_id: &str, title: &str, artwork_id: Option<&str>, playing: bool) -> WirePlayerState {
    WirePlayerState {
      track: Some(MediaItem {
        persistent_id: Some(persistent_id.to_string()),
        title: Some(title.to_string()),
        artwork_id: artwork_id.map(str::to_string),
        ..MediaItem::default()
      }),
      playback: Playback {
        state: if playing {
          PlaybackState::Playing
        } else {
          PlaybackState::Paused
        },
        ..Playback::default()
      },
      ..WirePlayerState::default()
    }
  }

  // an iap2 now-playing delta carrying the foreground app bundle, the signal the daemon gate reads.
  fn iap2_app(pid: &str, title: &str, app_bundle: &str, playing: bool) -> NowPlayingUpdate {
    NowPlayingUpdate {
      media_item: Some(MediaItemUpdate {
        persistent_id: Some(pid.to_string()),
        title: Some(title.to_string()),
        ..MediaItemUpdate::default()
      }),
      playback: Some(PlaybackUpdate {
        playing: Some(playing),
        app_bundle: Some(app_bundle.to_string()),
        ..PlaybackUpdate::default()
      }),
    }
  }

  fn artwork_id_of(state: &PlayerState) -> Option<String> {
    state.replies().0.state.track.and_then(|t| t.artwork_id)
  }

  #[test]
  fn iap2_now_playing_does_not_emit_artwork_id_until_apply_artwork_id() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_now_playing(iap2_track("iap2:track:abc", "Heart of Glass"));
    assert_eq!(artwork_id_of(&state), None);

    state.apply_artwork_id("iap2/art/abc/5".to_string());
    assert_eq!(artwork_id_of(&state), Some("iap2/art/abc/5".to_string()));
  }

  #[test]
  fn idle_persistent_id_zero_suppresses_track_emission() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_now_playing(NowPlayingUpdate {
      media_item: Some(MediaItemUpdate {
        persistent_id: Some("iap2:track:0000000000000000".to_string()),
        title: Some(String::new()),
        ..MediaItemUpdate::default()
      }),
      playback: None,
    });
    assert!(state.replies().0.state.track.is_none());
    assert_eq!(state.current_artwork_id(), None);
  }

  #[test]
  fn pid_zero_with_real_title_emits_track() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_now_playing(iap2_track(
      "iap2:track:0000000000000000",
      "99.9% Of Elden Ring Players CAN'T Beat This Mod",
    ));
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
    state.apply_now_playing(iap2_track("iap2:track:a", "Track A"));
    state.apply_artwork_id("iap2/art/a/5".to_string());
    assert_eq!(artwork_id_of(&state), Some("iap2/art/a/5".to_string()));

    state.apply_now_playing(iap2_track("iap2:track:b", "Track B"));
    assert_eq!(artwork_id_of(&state), None);
  }

  #[test]
  fn companion_authoritative_clears_iap2_art_on_wire() {
    let auth = AuthorityRegistry::new();
    let mut state = PlayerState::new(auth.clone());
    state.apply_now_playing(iap2_track("track:a", "A"));
    state.apply_artwork_id("iap2/art/a/5".to_string());
    assert_eq!(artwork_id_of(&state), Some("iap2/art/a/5".to_string()));

    auth.claim(CompanionAuthorityScope::NowPlayingMetadata);
    state.apply_companion_snapshot(companion_snapshot("track:a", "A", None, true));
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
    state.apply_now_playing(iap2_track("track:a", "A"));
    state.apply_artwork_id("iap2/art/a/5".to_string());

    auth.claim(CompanionAuthorityScope::NowPlayingMetadata);
    state.apply_companion_snapshot(companion_snapshot("track:a", "A", Some("spotify/track/a/image"), true));
    assert_eq!(artwork_id_of(&state), Some("spotify/track/a/image".to_string()));

    auth.release(CompanionAuthorityScope::NowPlayingMetadata);
    state.apply_now_playing(iap2_track("track:a", "A"));
    assert_eq!(artwork_id_of(&state), Some("iap2/art/a/5".to_string()));
  }

  #[test]
  fn current_artwork_id_filters_idle_track() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_now_playing(NowPlayingUpdate {
      media_item: Some(MediaItemUpdate {
        persistent_id: Some("iap2:track:0000000000000000".to_string()),
        title: Some(String::new()),
        ..MediaItemUpdate::default()
      }),
      playback: None,
    });
    state.apply_artwork_id("iap2/art/0000000000000000/1".to_string());
    assert_eq!(state.current_artwork_id(), None);
  }

  #[test]
  fn build_media_item_emits_none_for_empty_image_id() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_now_playing(iap2_track("track:a", "A"));
    let track = state.replies().0.state.track.expect("track present");
    assert_eq!(track.artwork_id, None);
  }

  fn qitem(uri: &str, title: &str, artist: &str, art: Option<&str>, duration_ms: Option<u32>) -> QueueItem {
    QueueItem {
      uri: uri.to_string(),
      title: Some(title.to_string()),
      artist: Some(artist.to_string()),
      artist_uri: None,
      album: None,
      album_uri: None,
      artwork_id: art.map(str::to_string),
      duration_ms,
      persistent_id: None,
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
  fn synthetic_iap2_identity_without_companion_is_non_actionable() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_now_playing(iap2_track("iap2:track:a", "Song"));
    let m = media(&state);
    assert_eq!(m.uri, None, "a synthetic iap2 identity exposes no actionable uri");
    assert_eq!(m.persistent_id.as_deref(), Some("iap2:track:a"));
    assert_eq!(m.artwork_id, None);
    assert_eq!(m.is_like_supported, None);
  }

  #[test]
  fn non_synthetic_companion_identity_is_actionable() {
    let auth = AuthorityRegistry::new();
    let mut state = PlayerState::new(auth.clone());
    auth.claim(CompanionAuthorityScope::NowPlayingMetadata);
    state.apply_companion_snapshot(companion_snapshot("spotify:track:x", "Song", None, true));
    let m = media(&state);
    assert_eq!(
      m.uri.as_deref(),
      Some("spotify:track:x"),
      "a real provider uri is actionable"
    );
  }

  #[test]
  fn synthetic_uri_predicate() {
    assert!(is_synthetic_uri("iap2:track:abc"));
    assert!(!is_synthetic_uri("spotify:track:abc"));
  }

  #[test]
  fn companion_snapshot_drives_now_playing_when_authoritative() {
    let auth = AuthorityRegistry::new();
    let mut state = PlayerState::new(auth.clone());
    auth.claim(CompanionAuthorityScope::NowPlayingMetadata);
    auth.claim(CompanionAuthorityScope::NowPlayingPlayback);
    auth.set_companion_app_bundle(Some("com.spotify.client".into()));
    state.apply_companion_snapshot(companion_snapshot(
      "spotify:track:x",
      "Spotify Song",
      Some("spotify/img/x"),
      true,
    ));

    let m = media(&state);
    assert_eq!(m.uri.as_deref(), Some("spotify:track:x"));
    assert_eq!(m.title.as_deref(), Some("Spotify Song"));
    assert_eq!(m.artwork_id.as_deref(), Some("spotify/img/x"));
    assert_eq!(state.replies().0.state.playback.state, PlaybackState::Playing);
  }

  #[test]
  fn iap2_other_foreground_app_overrides_companion_authority() {
    let auth = AuthorityRegistry::new();
    let mut state = PlayerState::new(auth.clone());
    auth.claim(CompanionAuthorityScope::NowPlayingMetadata);
    auth.claim(CompanionAuthorityScope::NowPlayingPlayback);
    auth.set_companion_app_bundle(Some("com.spotify.client".into()));
    state.apply_companion_snapshot(companion_snapshot(
      "spotify:track:x",
      "Spotify Song",
      Some("spotify/img/x"),
      true,
    ));

    // iap2 reports spotify as the foreground app: the companion stays authoritative.
    state.apply_now_playing(iap2_app("iap2:track:s", "Spotify Song", "com.spotify.client", true));
    assert_eq!(
      media(&state).uri.as_deref(),
      Some("spotify:track:x"),
      "same-app iap2 does not override"
    );

    // iap2 reports youtube as the foreground app: the still-claimed companion is overridden.
    state.apply_now_playing(iap2_app(
      "iap2:track:y",
      "YouTube Video",
      "com.google.ios.youtube",
      true,
    ));
    let m = media(&state);
    assert_eq!(
      m.title.as_deref(),
      Some("YouTube Video"),
      "a diverging foreground bundle hands now-playing to iap2"
    );
    assert_eq!(m.persistent_id.as_deref(), Some("iap2:track:y"));
    assert_eq!(m.uri, None, "the iap2 identity is synthetic");

    // the user returns to spotify: the companion re-takes without a fresh snapshot.
    state.apply_now_playing(iap2_app("iap2:track:s2", "Spotify Song", "com.spotify.client", true));
    assert_eq!(
      media(&state).uri.as_deref(),
      Some("spotify:track:x"),
      "companion re-takes on the bundle returning"
    );
  }

  #[test]
  fn active_app_surfaces_only_for_non_spotify_now_playing() {
    let auth = AuthorityRegistry::new();
    let mut state = PlayerState::new(auth.clone());
    auth.claim(CompanionAuthorityScope::NowPlayingMetadata);
    auth.claim(CompanionAuthorityScope::NowPlayingPlayback);
    auth.set_companion_app_bundle(Some("com.spotify.client".into()));
    state.apply_companion_snapshot(companion_snapshot(
      "spotify:track:x",
      "Spotify Song",
      Some("spotify/img/x"),
      true,
    ));
    assert!(state.active_app().is_none(), "spotify now-playing is not other-media");

    state.apply_now_playing(iap2_app(
      "iap2:track:y",
      "YouTube Video",
      "com.google.ios.youtube",
      true,
    ));
    let app = state.active_app().expect("other-media app present");
    assert_eq!(app.id, "com.google.ios.youtube");
  }

  fn position_tick(position_ms: u32) -> NowPlayingUpdate {
    NowPlayingUpdate {
      media_item: None,
      playback: Some(PlaybackUpdate {
        position_ms: Some(position_ms),
        ..PlaybackUpdate::default()
      }),
    }
  }

  fn playing_track(pid: &str, position_ms: u32) -> NowPlayingUpdate {
    NowPlayingUpdate {
      media_item: Some(MediaItemUpdate {
        persistent_id: Some(pid.into()),
        title: Some("Song".into()),
        duration_ms: Some(180_000),
        ..MediaItemUpdate::default()
      }),
      playback: Some(PlaybackUpdate {
        playing: Some(true),
        position_ms: Some(position_ms),
        ..PlaybackUpdate::default()
      }),
    }
  }

  #[test]
  fn routine_progress_tick_does_not_force_resync() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_now_playing(playing_track("iap2:track:a", 1_000));
    assert!(state.take_position_resync(), "first position of a track resyncs");

    state.apply_now_playing(position_tick(1_200));
    assert!(
      !state.take_position_resync(),
      "a tick near the extrapolated playhead rides without a broadcast"
    );
  }

  #[test]
  fn position_jump_forces_resync() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_now_playing(playing_track("iap2:track:a", 1_000));
    state.take_position_resync();

    state.apply_now_playing(position_tick(120_000));
    assert!(
      state.take_position_resync(),
      "a phone-side seek beyond tolerance resyncs the webapp"
    );
  }

  #[test]
  fn backward_position_jitter_while_riding_is_ignored() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_now_playing(playing_track("iap2:track:a", 1_000));
    state.take_position_resync();

    state.apply_now_playing(position_tick(1_500));
    let forward = state.replies().0.state.playback.position_ms;

    // a stale out-of-order backward tick within tolerance must not snap the playhead back.
    state.apply_now_playing(position_tick(900));
    assert!(!state.take_position_resync(), "backward jitter does not force a resync");
    let after = state.replies().0.state.playback.position_ms;
    assert!(
      after >= forward,
      "playhead never regresses on backward jitter: {after} < {forward}"
    );
  }

  #[test]
  fn position_tick_while_paused_resyncs() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_now_playing(playing_track("iap2:track:a", 1_000));
    state.take_position_resync();
    state.apply_now_playing(NowPlayingUpdate {
      media_item: None,
      playback: Some(PlaybackUpdate {
        playing: Some(false),
        ..PlaybackUpdate::default()
      }),
    });
    state.take_position_resync();

    // scrub while paused: no clock to ride, so the change must be pushed.
    state.apply_now_playing(position_tick(5_000));
    assert!(state.take_position_resync(), "paused scrub resyncs");
  }

  #[test]
  fn companion_queue_rebuilds_order_from_snapshot_items() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    let a = qitem("spotify:track:a", "A", "X", Some("img/a"), Some(1000));
    let b = qitem("spotify:track:b", "B", "X", Some("img/b"), Some(2000));
    let c = qitem("spotify:track:c", "C", "X", Some("img/c"), Some(3000));

    state.apply_companion_queue(qsnap(vec![a.clone(), b.clone(), c.clone()]));
    assert_eq!(state.companion_queue, vec![a, b.clone(), c.clone()]);

    // each snapshot fully replaces: items carries every uri in order, so the queue is rebuilt from
    // the snapshot alone with no carry-over from the prior one.
    let d = qitem("spotify:track:d", "D", "X", Some("img/d"), Some(4000));
    state.apply_companion_queue(qsnap(vec![b.clone(), c.clone(), d.clone()]));
    assert_eq!(state.companion_queue, vec![b, c, d]);
  }

  #[test]
  fn derive_next_strips_current_when_held_queue_still_contains_it() {
    let b = qitem("spotify:track:b", "B", "X", None, None);
    let c = qitem("spotify:track:c", "C", "X", None, None);
    let d = qitem("spotify:track:d", "D", "X", None, None);
    // after a plain advance the companion sends nothing, so the now-current track is still the head
    // of the held upcoming; next is the suffix after it.
    let next = derive_next(vec![b.clone(), c.clone(), d.clone()], Some(&b));
    assert_eq!(next, vec![c, d]);
  }

  #[test]
  fn derive_next_returns_all_when_current_absent() {
    let b = qitem("spotify:track:b", "B", "X", None, None);
    let c = qitem("spotify:track:c", "C", "X", None, None);
    let current = qitem("spotify:track:a", "A", "X", None, None);
    // a fresh snapshot excludes the current track, so it is not found and the whole list is next.
    let next = derive_next(vec![b.clone(), c.clone()], Some(&current));
    assert_eq!(next, vec![b, c]);
  }

  #[test]
  fn recently_played_pushes_front_and_gates_home_gen_on_duration() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    let a = qitem("spotify:track:a", "A", "X", None, None);
    let b = qitem("spotify:track:b", "B", "X", None, None);

    state.note_rolled_off(a.clone(), 5_000);
    assert_eq!(state.recently_played, vec![a.clone()]);
    assert_eq!(
      state.recently_played_gen(),
      0,
      "a sub-30s play does not invalidate home"
    );

    state.note_rolled_off(b.clone(), 45_000);
    assert_eq!(state.recently_played, vec![b, a], "most-recent-first");
    assert_eq!(state.recently_played_gen(), 1, "a >=30s play invalidates home");
  }

  #[test]
  fn reset_companion_clears_queue_and_bumps_home_gen() {
    let mut state = PlayerState::new(AuthorityRegistry::new());
    state.apply_companion_queue(qsnap(vec![qitem("spotify:track:a", "A", "X", None, None)]));
    state.note_rolled_off(qitem("spotify:track:b", "B", "X", None, None), 45_000);
    let before = state.recently_played_gen();

    state.reset_companion();
    assert!(state.companion_queue.is_empty());
    assert!(state.recently_played.is_empty());
    assert_eq!(state.recently_played_gen(), before + 1);
  }
}
