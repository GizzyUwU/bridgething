use libbridgething::{
  CompanionAuthorityScope, MediaItem, MediaItemUpdate, NowPlayingUpdate, Playback, PlaybackOptions, PlaybackState,
  PlaybackUpdate, PlayerOptions, PlayerState as WirePlayerState, QueueItem, Track,
  client::{PlayerQueueReply, PlayerStateReply},
};

use crate::authority::AuthorityRegistry;

/// Which producer fed an inbound `NowPlayingUpdate`. The merge stage
/// uses this to route the partial fields into the right source-snapshot
/// before recomputing the merged surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NowPlayingSource {
  /// iAP2 control session - iOS-side ground truth, always available
  /// when an iPhone is connected and Identified.
  Iap2,
  /// bridgething companion app over RFCOMM (Android) or iAP2 EA (iOS).
  /// Authority over a scope is tracked separately in
  /// `AuthorityRegistry`.
  Companion,
}

#[derive(Debug, Clone)]
pub struct PlayerState {
  authority: AuthorityRegistry,

  pub playing: bool,

  pub context_title: String,
  pub context_id: Option<String>,

  pub position_ms: usize,
  pub playback_speed: f64,

  pub track: Option<Track>,

  pub options: PlaybackOptions,

  iap2_metadata: MediaItemUpdate,
  iap2_playback: PlaybackUpdate,
  companion_metadata: MediaItemUpdate,
  companion_playback: PlaybackUpdate,

  iap2_queue: Vec<QueueItem>,
}

impl PlayerState {
  pub fn new(authority: AuthorityRegistry) -> Self {
    Self {
      authority,

      playing: false,

      context_title: "BridgeThing".to_string(),
      context_id: None,

      position_ms: 0,
      playback_speed: 1.0,

      track: None,

      options: PlaybackOptions::default(),

      iap2_metadata: MediaItemUpdate::default(),
      iap2_playback: PlaybackUpdate::default(),
      companion_metadata: MediaItemUpdate::default(),
      companion_playback: PlaybackUpdate::default(),

      iap2_queue: Vec::new(),
    }
  }

  pub(crate) fn replace_iap2_queue(&mut self, items: Vec<QueueItem>) {
    self.iap2_queue = items;
  }

  fn merged_queue(&self) -> Vec<QueueItem> {
    self.iap2_queue.clone()
  }

  pub(crate) fn apply_now_playing(&mut self, source: NowPlayingSource, update: NowPlayingUpdate) {
    let NowPlayingUpdate { media_item, playback } = update;

    let (meta_target, play_target) = match source {
      NowPlayingSource::Companion => (&mut self.companion_metadata, &mut self.companion_playback),
      NowPlayingSource::Iap2 => (&mut self.iap2_metadata, &mut self.iap2_playback),
    };

    if let Some(media) = media_item {
      accumulate_media(meta_target, media);
    }
    if let Some(play) = playback {
      accumulate_playback(play_target, play);
    }

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
        artwork_id: self
          .companion_metadata
          .artwork_id
          .clone()
          .or_else(|| self.iap2_metadata.artwork_id.clone()),
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
    if let Some(image_id) = media.artwork_id {
      track.image_id = image_id;
    }
    if let Some(duration) = media.duration_ms {
      track.duration_ms = duration;
    }
    if let Some(liked) = media.liked {
      track.saved = liked;
    }
    self.track = Some(track);

    if let Some(playing) = playback.playing {
      self.playing = playing;
    }
    if let Some(position) = playback.position_ms {
      self.position_ms = position as usize;
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

  pub fn iap2_playback_snapshot(&self) -> PlaybackUpdate {
    self.iap2_playback.clone()
  }

  pub fn state_reply(&self) -> PlayerStateReply {
    let merged = self.merged_playback();
    let merged_meta = self.merged_metadata();
    PlayerStateReply {
      state: WirePlayerState {
        track: self.track.as_ref().map(|t| build_media_item(t, &merged_meta)),
        playback: Playback {
          state: if self.playing {
            PlaybackState::Playing
          } else {
            PlaybackState::Paused
          },
          position_ms: u32::try_from(self.position_ms).unwrap_or(u32::MAX),
          shuffle: self.options.shuffle,
          shuffle_mode: merged.shuffle_mode,
          repeat: self.options.repeat,
          queue_index: merged.queue_index,
          queue_count: merged.queue_count,
          queue_chapter_index: merged.queue_chapter_index,
          set_elapsed_time_available: merged.set_elapsed_time_available,
          queue_list_avail: merged.queue_list_avail,
          apple_music_radio_ad: merged.apple_music_radio_ad,
        },
        queue: self.merged_queue(),
        options: PlayerOptions {
          speed: merged.playback_speed.unwrap_or(self.playback_speed as f32),
          crossfade_ms: None,
        },
      },
    }
  }

  pub fn queue_reply(&self) -> PlayerQueueReply {
    PlayerQueueReply {
      items: self.merged_queue(),
    }
  }
}

fn build_media_item(track: &Track, merged: &MediaItemUpdate) -> MediaItem {
  MediaItem {
    uri: None,
    persistent_id: Some(track.id.clone()),
    title: Some(track.name.clone()),
    album: Some(track.album.name.clone()),
    album_artist: merged.album_artist.clone(),
    artist: Some(track.artist.name.clone()),
    liked: Some(track.saved),
    artwork_id: Some(track.image_id.clone()),
    duration_ms: Some(track.duration_ms),
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
