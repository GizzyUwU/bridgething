use libbridgething::{
  CompanionAuthorityScope, MediaItem, MediaItemUpdate, NowPlayingUpdate, Playback, PlaybackOptions, PlaybackState,
  PlaybackUpdate, PlayerOptions, PlayerState as WirePlayerState, Track,
  client::{BridgeToClientPlayerMsg, PlayerQueueReply, PlayerStateReply},
  wire::MsgMeta,
};

use super::PlayerResult;
use crate::{authority::AuthorityRegistry, net::ClientMan};

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
  client_man: ClientMan,
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
}

impl PlayerState {
  pub fn new(client_man: ClientMan, authority: AuthorityRegistry) -> Self {
    Self {
      client_man,
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
    }
  }

  pub(crate) async fn apply_now_playing(
    &mut self,
    source: NowPlayingSource,
    update: NowPlayingUpdate,
  ) -> PlayerResult<()> {
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
    self.send_state().await
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

  pub async fn send_state(&self) -> PlayerResult<()> {
    self.client_man.broadcast(self.to_send_state(), MsgMeta::Event).await?;
    self.client_man.broadcast(self.to_send_queue(), MsgMeta::Event).await?;

    Ok(())
  }

  pub fn to_send_state(&self) -> BridgeToClientPlayerMsg {
    BridgeToClientPlayerMsg::Snapshot(PlayerStateReply {
      state: WirePlayerState {
        track: self.track.as_ref().map(track_to_media_item),
        playback: Playback {
          state: if self.playing {
            PlaybackState::Playing
          } else {
            PlaybackState::Paused
          },
          position_ms: u32::try_from(self.position_ms).unwrap_or(u32::MAX),
          shuffle: self.options.shuffle,
          repeat: self.options.repeat,
        },
        queue: vec![],
        options: PlayerOptions {
          speed: self.playback_speed as f32,
          crossfade_ms: None,
        },
      },
    })
  }

  pub fn to_send_queue(&self) -> BridgeToClientPlayerMsg {
    BridgeToClientPlayerMsg::QueueChanged(PlayerQueueReply { items: vec![] })
  }
}

fn track_to_media_item(track: &Track) -> MediaItem {
  MediaItem {
    uri: None,
    persistent_id: Some(track.id.clone()),
    title: Some(track.name.clone()),
    album: Some(track.album.name.clone()),
    artist: Some(track.artist.name.clone()),
    liked: Some(track.saved),
    artwork_id: Some(track.image_id.clone()),
    duration_ms: Some(track.duration_ms),
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
  if src.repeat.is_some() {
    target.repeat = src.repeat;
  }
  if src.app_bundle.is_some() {
    target.app_bundle = src.app_bundle;
  }
  if src.app_display_name.is_some() {
    target.app_display_name = src.app_display_name;
  }
}
