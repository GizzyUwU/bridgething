//! Reference merge model: a deliberately-simple, time-frozen re-implementation
//! of the iAP2 event router (the pid-hex pairing + pending-art bookkeeping in
//! `handler/iap2.rs`) plus the player merge (`player/state.rs`), used as the
//! oracle for the property engine.
//!
//! The model is the spec written twice: if it and the daemon disagree after the
//! same event sequence, one of them has a bug. To stay an honest oracle it is
//! deliberately naive (no actors, no async, obviously-correct by inspection) and
//! it freezes time out of scope:
//!
//! - authority claims never go stale (the daemon expires them after 5s; the
//!   property engine runs sub-second, so a live claim stays live),
//! - no transport/seek intent windows are ever armed (the vocabulary is
//!   inbound-only; those are set by outbound control commands),
//! - the extrapolated `position_ms` is not modeled and is excluded from the
//!   compared projection.
//!
//! Comparison is at a curated semantic [`Projection`], not a full
//! `PlayerStateReply`: every distinct merge rule (Some-overwrite accumulation,
//! track-change reset of the metadata accumulator, per-scope authority
//! fallthrough with the artwork-no-fallthrough exception, idle-sentinel
//! suppression, and transfer-id art pairing) drives at least one projected
//! field. Fields that are merely carried through by the same rule as a projected
//! field add maintenance cost without coverage.

use std::collections::HashSet;

use bridgething_iap2::csm::now_playing::{
  MediaItemAttributes, NowPlayingUpdate as Iap2NowPlaying, PlaybackAttributes, PlaybackState as Iap2PlaybackState,
  RepeatMode as Iap2Repeat, ShuffleMode as Iap2Shuffle,
};
use libbridgething::{
  CompanionAuthorityScope, MediaItemUpdate, PlaybackState, PlaybackUpdate, PlayerState, RepeatMode, ShuffleMode,
  client::{PlayerQueueReply, PlayerStateReply},
};

const IDLE_PID_HEX: &str = "0000000000000000";
const NONMUSIC_PREFIX: &str = "nonmusic-";

/// One atomic inbound event, applied identically to the model and (via the
/// harness drivers) to the daemon.
///
/// `AuthorityClaim`/`AuthorityRelease` flip the registry but, like the daemon,
/// do NOT refresh the player projection: a bare claim is latent until the next
/// player command recomputes the merge against the new authority.
#[derive(Debug, Clone)]
pub enum ModelEvent {
  Iap2NowPlaying(Iap2NowPlaying),
  Iap2Artwork { transfer_id: u8, bytes_len: usize },
  CompanionSnapshot(PlayerState),
  AuthorityClaim(CompanionAuthorityScope),
  AuthorityRelease(CompanionAuthorityScope),
}

/// The curated semantic projection compared between model and daemon. Excludes
/// the wall-clock-extrapolated `position_ms` by construction.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Projection {
  pub track_id: Option<String>,
  pub title: Option<String>,
  pub album: Option<String>,
  pub artist: Option<String>,
  pub album_artist: Option<String>,
  pub wire_artwork_id: Option<String>,
  pub current_artwork_id: Option<String>,
  pub duration_ms: Option<u32>,
  pub liked: Option<bool>,
  pub playing: bool,
  pub shuffle: bool,
  pub shuffle_mode: Option<ShuffleMode>,
  pub repeat: RepeatMode,
  pub queue_count: Option<u32>,
  pub queue_index: Option<u32>,
  pub set_elapsed_time_available: Option<bool>,
  pub queue_len: usize,
  pub queue_first_id: Option<String>,
}

impl Projection {
  /// Build the projection from what the daemon actually exposes for assertions.
  pub fn from_daemon(state: &PlayerStateReply, queue: &PlayerQueueReply, current_artwork_id: Option<String>) -> Self {
    let track = state.state.track.as_ref();
    Self {
      track_id: track.and_then(|t| t.persistent_id.clone()),
      title: track.and_then(|t| t.title.clone()),
      album: track.and_then(|t| t.album.clone()),
      artist: track.and_then(|t| t.artist.clone()),
      album_artist: track.and_then(|t| t.album_artist.clone()),
      wire_artwork_id: track.and_then(|t| t.artwork_id.clone()),
      current_artwork_id,
      duration_ms: track.and_then(|t| t.duration_ms),
      liked: track.and_then(|t| t.liked),
      playing: matches!(state.state.playback.state, libbridgething::PlaybackState::Playing),
      shuffle: state.state.playback.shuffle,
      shuffle_mode: state.state.playback.shuffle_mode,
      repeat: state.state.playback.repeat,
      queue_count: state.state.playback.queue_count,
      queue_index: state.state.playback.queue_index,
      set_elapsed_time_available: state.state.playback.set_elapsed_time_available,
      queue_len: queue.items.len(),
      queue_first_id: queue.items.first().map(|i| i.uri.clone()),
    }
  }
}

/// The reference merge state. Mirrors the daemon's per-source accumulators, the
/// router's iAP2 pairing bookkeeping, and the authority registry (as a plain set
/// since claims never go stale in the engine's time window).
#[derive(Debug, Clone, Default)]
pub struct Model {
  // router (handler/iap2.rs) bookkeeping, single peer
  last_pid_hex: Option<String>,
  pending_art: Option<(u8, String)>,
  assets: HashSet<String>,

  // player accumulators (player/state.rs)
  iap2_metadata: MediaItemUpdate,
  iap2_playback: PlaybackUpdate,
  companion_metadata: MediaItemUpdate,
  companion_playback: PlaybackUpdate,
  companion_queue: Vec<QueueEntry>,

  authority: HashSet<CompanionAuthorityScope>,

  // merged track (only the fields the projection reads)
  track: Option<TrackModel>,
  playing: bool,
  shuffle: bool,
  repeat: RepeatMode,

  // the daemon's player snapshot is recomputed only on player-actor commands, so
  // the observable projection is cached and refreshed there - never on a bare
  // authority change. mirror that exactly.
  cached: Projection,
}

#[derive(Debug, Clone)]
struct QueueEntry {
  uri: String,
}

#[derive(Debug, Clone, Default)]
struct TrackModel {
  id: String,
  name: String,
  album: String,
  artist: String,
  image_id: String,
  duration_ms: u32,
  saved: bool,
}

impl Model {
  pub fn new() -> Self {
    // the daemon's initial PlayerState has no track and does not run the merge
    // until the first player command, so seed the cache from the empty state
    // (track None) WITHOUT synthesizing the default track recompute would build.
    let mut model = Self::default();
    model.cached = model.build_projection();
    model
  }

  pub fn apply(&mut self, event: &ModelEvent) {
    match event {
      ModelEvent::Iap2NowPlaying(update) => self.apply_iap2_now_playing(update),
      ModelEvent::Iap2Artwork { transfer_id, .. } => self.apply_iap2_artwork(*transfer_id),
      ModelEvent::CompanionSnapshot(snapshot) => self.apply_companion_snapshot(snapshot),
      ModelEvent::AuthorityClaim(scope) => {
        self.authority.insert(*scope);
      }
      ModelEvent::AuthorityRelease(scope) => {
        self.authority.remove(scope);
      }
    }
  }

  // mirror of the daemon's player/state.rs::apply_companion_snapshot: a snapshot is a full replace
  // of the companion accumulators (never an accumulate), and never touches the companion queue
  // (that rides QueueChanged).
  fn apply_companion_snapshot(&mut self, snapshot: &PlayerState) {
    let PlayerState {
      track,
      playback,
      options,
      ..
    } = snapshot;

    self.companion_metadata = match track {
      Some(t) => MediaItemUpdate {
        persistent_id: t.persistent_id.clone(),
        title: t.title.clone(),
        album: t.album.clone(),
        album_uri: t.album_uri.clone(),
        album_artist: t.album_artist.clone(),
        artist: t.artist.clone(),
        artist_uri: t.artist_uri.clone(),
        liked: t.liked,
        artwork_id: t.artwork_id.clone(),
        duration_ms: t.duration_ms,
        media_types: t.media_types.clone(),
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

    self.recompute();
  }

  // --- router half (handler/iap2.rs) ---

  fn apply_iap2_now_playing(&mut self, update: &Iap2NowPlaying) {
    let pid_hex = match delta_track_key(update.media_item.as_ref(), self.last_pid_hex.as_deref()) {
      Some(key) => {
        if self.last_pid_hex.as_deref() != Some(&key) {
          self.pending_art = None;
        }
        self.last_pid_hex = Some(key.clone());
        Some(key)
      }
      None => self.last_pid_hex.clone(),
    };

    if let Some(hex) = pid_hex.as_deref()
      && hex != IDLE_PID_HEX
      && let Some(transfer_id) = update.media_item.as_ref().and_then(|m| m.artwork_id)
    {
      self.pending_art = Some((transfer_id, format!("iap2/art/{hex}/{transfer_id}")));
    }

    let lib_update = translate_now_playing(update, pid_hex.as_deref());
    self.player_apply_now_playing(lib_update);
  }

  fn apply_iap2_artwork(&mut self, transfer_id: u8) {
    let asset_id = match &self.pending_art {
      Some((tid, asset_id)) if *tid == transfer_id => {
        let asset_id = asset_id.clone();
        self.pending_art = None;
        asset_id
      }
      _ => return,
    };
    self.assets.insert(asset_id.clone());
    self.player_apply_artwork_id(asset_id);
  }

  // --- player half (player/state.rs) ---

  // iap2 is the only producer of now-playing deltas; the companion drives now-playing through
  // apply_companion_snapshot, so this always targets the iap2 accumulators.
  fn player_apply_now_playing(&mut self, update: (MediaItemUpdate, PlaybackUpdate, bool, bool)) {
    let (mut media, playback, has_media, mut has_playback) = update;

    // mirrors the daemon's idle-sentinel ingest drop: a zero-pid empty-title delta is an iOS
    // transition blip; only a real duration riding on it survives, everything else is dropped
    // without touching the accumulators.
    let is_idle_sentinel = has_media
      && media
        .persistent_id
        .as_deref()
        .is_some_and(|p| p.ends_with(IDLE_PID_HEX))
      && media.title.as_deref() == Some("");
    if is_idle_sentinel {
      has_playback = false;
      match media.duration_ms.filter(|d| *d > 0) {
        Some(duration) => {
          media = MediaItemUpdate {
            duration_ms: Some(duration),
            ..MediaItemUpdate::default()
          };
        }
        None => {
          // the daemon drops the pure sentinel without touching state, but the player actor
          // still rebuilds its watch snapshot on every command, so live-merged fields (e.g. an
          // out-of-band claim flipping the merge) surface. the daemon's wire art is derived
          // from the live merge at snapshot build, so the mirrored track image resyncs too.
          let merged_art = self.merged_metadata().artwork_id.unwrap_or_default();
          if let Some(track) = self.track.as_mut() {
            track.image_id = merged_art;
          }
          self.cached = self.build_projection();
          return;
        }
      }
    }
    let (meta_target, play_target) = (&mut self.iap2_metadata, &mut self.iap2_playback);
    if has_media {
      if let Some(new_pid) = media.persistent_id.as_ref()
        && meta_target.persistent_id.as_ref() != Some(new_pid)
      {
        *meta_target = MediaItemUpdate::default();
        play_target.position_ms = Some(0);
      }
      accumulate_media(meta_target, media);
    }
    if has_playback {
      accumulate_playback(play_target, playback);
    }
    self.recompute();
  }

  fn player_apply_artwork_id(&mut self, asset_id: String) {
    self.iap2_metadata.artwork_id = Some(asset_id);
    self.recompute();
  }

  fn companion_authoritative(&self, scope: CompanionAuthorityScope) -> bool {
    self.authority.contains(&scope)
  }

  fn merged_metadata(&self) -> MediaItemUpdate {
    if self.companion_authoritative(CompanionAuthorityScope::NowPlayingMetadata) {
      let c = &self.companion_metadata;
      let i = &self.iap2_metadata;
      MediaItemUpdate {
        persistent_id: c.persistent_id.clone().or_else(|| i.persistent_id.clone()),
        title: c.title.clone().or_else(|| i.title.clone()),
        album: c.album.clone().or_else(|| i.album.clone()),
        album_uri: c.album_uri.clone().or_else(|| i.album_uri.clone()),
        album_artist: c.album_artist.clone().or_else(|| i.album_artist.clone()),
        artist: c.artist.clone().or_else(|| i.artist.clone()),
        artist_uri: c.artist_uri.clone().or_else(|| i.artist_uri.clone()),
        liked: c.liked.or(i.liked),
        artwork_id: c.artwork_id.clone(),
        duration_ms: c.duration_ms.or(i.duration_ms),
        media_types: c.media_types.clone().or_else(|| i.media_types.clone()),
        track_number: c.track_number.or(i.track_number),
        track_count: c.track_count.or(i.track_count),
        is_like_supported: c.is_like_supported.or(i.is_like_supported),
        is_ban_supported: c.is_ban_supported.or(i.is_ban_supported),
        is_banned: c.is_banned.or(i.is_banned),
        is_resident_on_device: c.is_resident_on_device.or(i.is_resident_on_device),
        chapter_count: c.chapter_count.or(i.chapter_count),
      }
    } else {
      self.iap2_metadata.clone()
    }
  }

  fn merged_playback(&self) -> PlaybackUpdate {
    if self.companion_authoritative(CompanionAuthorityScope::NowPlayingPlayback) {
      let c = &self.companion_playback;
      let i = &self.iap2_playback;
      PlaybackUpdate {
        playing: c.playing.or(i.playing),
        position_ms: c.position_ms.or(i.position_ms),
        shuffle: c.shuffle.or(i.shuffle),
        shuffle_mode: c.shuffle_mode.or(i.shuffle_mode),
        repeat: c.repeat.or(i.repeat),
        app_bundle: c.app_bundle.clone().or_else(|| i.app_bundle.clone()),
        app_display_name: c.app_display_name.clone().or_else(|| i.app_display_name.clone()),
        queue_index: c.queue_index.or(i.queue_index),
        queue_count: c.queue_count.or(i.queue_count),
        queue_chapter_index: c.queue_chapter_index.or(i.queue_chapter_index),
        playback_speed: c.playback_speed.or(i.playback_speed),
        set_elapsed_time_available: c.set_elapsed_time_available.or(i.set_elapsed_time_available),
        queue_list_avail: c.queue_list_avail.or(i.queue_list_avail),
        apple_music_radio_ad: c.apple_music_radio_ad.or(i.apple_music_radio_ad),
        apple_music_radio_station_name: c
          .apple_music_radio_station_name
          .clone()
          .or_else(|| i.apple_music_radio_station_name.clone()),
      }
    } else {
      self.iap2_playback.clone()
    }
  }

  fn merged_queue(&self) -> &[QueueEntry] {
    if self.companion_authoritative(CompanionAuthorityScope::NowPlayingMetadata) {
      &self.companion_queue
    } else {
      &[]
    }
  }

  fn recompute(&mut self) {
    let media = self.merged_metadata();
    let playback = self.merged_playback();

    // mirrors the daemon: an identity-free merge on an empty state does not fabricate a track
    let has_identity = media.persistent_id.is_some() || media.title.is_some();
    if self.track.is_some() || has_identity {
      let same_track = match (
        self.track.as_ref().map(|t| t.id.as_str()),
        media.persistent_id.as_deref(),
      ) {
        (Some(existing), Some(new)) => existing == new,
        _ => false,
      };
      let mut track = if same_track {
        self.track.clone().unwrap_or_default()
      } else {
        default_track()
      };
      if let Some(id) = media.persistent_id {
        track.id = id;
      }
      if let Some(title) = media.title {
        track.name = title;
      }
      if let Some(album) = media.album {
        track.album = album;
      }
      if let Some(artist) = media.artist {
        track.artist = artist;
      }
      track.image_id = media.artwork_id.unwrap_or_default();
      if let Some(duration) = media.duration_ms {
        track.duration_ms = duration;
      }
      if let Some(liked) = media.liked {
        track.saved = liked;
      }
      self.track = Some(track);
    }

    if let Some(playing) = playback.playing {
      self.playing = playing;
    }
    if let Some(shuffle) = playback.shuffle {
      self.shuffle = shuffle;
    }
    if let Some(repeat) = playback.repeat {
      self.repeat = repeat;
    }

    self.cached = self.build_projection();
  }

  fn effective_track(&self) -> Option<&TrackModel> {
    let track = self.track.as_ref()?;
    if track.id.ends_with(IDLE_PID_HEX) && track.name.is_empty() {
      return None;
    }
    Some(track)
  }

  fn build_projection(&self) -> Projection {
    let merged_meta = self.merged_metadata();
    let merged_play = self.merged_playback();
    let effective = self.effective_track();

    let wire_artwork_id = effective.and_then(|t| {
      if t.image_id.is_empty() {
        None
      } else {
        Some(t.image_id.clone())
      }
    });
    let current_artwork_id = effective
      .and_then(|_| merged_meta.artwork_id.clone())
      .filter(|s| !s.is_empty());

    let queue = self.merged_queue();

    Projection {
      track_id: effective.map(|t| t.id.clone()),
      title: effective.and(merged_meta.title.clone()),
      album: effective.and(merged_meta.album.clone()),
      artist: effective.and(merged_meta.artist.clone()),
      album_artist: effective.and(merged_meta.album_artist.clone()),
      wire_artwork_id,
      current_artwork_id,
      duration_ms: effective.and(merged_meta.duration_ms),
      liked: effective.and(merged_meta.liked),
      playing: self.playing,
      shuffle: self.shuffle,
      shuffle_mode: merged_play.shuffle_mode,
      repeat: self.repeat,
      queue_count: merged_play.queue_count,
      queue_index: merged_play.queue_index,
      set_elapsed_time_available: merged_play.set_elapsed_time_available,
      queue_len: queue.len(),
      queue_first_id: queue.first().map(|e| e.uri.clone()),
    }
  }

  /// The cached projection, refreshed only by player-actor commands (mirroring
  /// the daemon's watch snapshot). A bare authority change does not refresh it.
  pub fn project(&self) -> Projection {
    self.cached.clone()
  }

  /// The scopes the model currently considers authoritative. The barrier waits
  /// for the daemon's live scopes to match this, since a bare authority claim
  /// has no player-observable effect to converge on.
  pub fn authority_scopes(&self) -> HashSet<CompanionAuthorityScope> {
    self.authority.clone()
  }

  /// Asset ids the model believes are inserted or pending. Used by the
  /// dangling-artwork invariant: any projected art id must be in this set.
  pub fn known_asset_ids(&self) -> HashSet<String> {
    let mut ids = self.assets.clone();
    if let Some((_, asset_id)) = &self.pending_art {
      ids.insert(asset_id.clone());
    }
    ids
  }
}

fn default_track() -> TrackModel {
  TrackModel::default()
}

/// Translate one raw iAP2 NowPlaying delta the way `handler/iap2.rs` does,
/// returning the per-section lib updates plus presence flags (so the player
/// half can distinguish "no media group" from "media group with all-None").
fn translate_now_playing(
  update: &Iap2NowPlaying,
  persistent_hex: Option<&str>,
) -> (MediaItemUpdate, PlaybackUpdate, bool, bool) {
  let media = update
    .media_item
    .as_ref()
    .map(|m| translate_media_item(m, persistent_hex));
  let playback = update.playback.as_ref().map(translate_playback);
  (
    media.clone().unwrap_or_default(),
    playback.clone().unwrap_or_default(),
    media.is_some(),
    playback.is_some(),
  )
}

// mirror of the daemon's `handler/iap2.rs::delta_track_key` - the spec written twice.
fn delta_track_key(media: Option<&MediaItemAttributes>, current: Option<&str>) -> Option<String> {
  let media = media?;
  let title = media.title.as_deref().filter(|t| !t.is_empty());
  match media.persistent_id {
    Some(pid) if pid != 0 => Some(format!("{pid:016x}")),
    _ if title.is_some() && current.is_some_and(is_real_pid_key) => current.map(str::to_string),
    _ if title.is_some() => Some(nonmusic_key(title.unwrap(), media.artist.as_deref())),
    Some(0) => Some(IDLE_PID_HEX.to_string()),
    _ => None,
  }
}

fn is_real_pid_key(key: &str) -> bool {
  key.len() == 16 && key != IDLE_PID_HEX && key.bytes().all(|b| b.is_ascii_hexdigit())
}

fn nonmusic_key(title: &str, artist: Option<&str>) -> String {
  use std::hash::{DefaultHasher, Hash, Hasher};
  let mut hasher = DefaultHasher::new();
  title.hash(&mut hasher);
  if let Some(a) = artist {
    a.hash(&mut hasher);
  }
  format!("{NONMUSIC_PREFIX}{:016x}", hasher.finish())
}

fn translate_media_item(media: &MediaItemAttributes, track_key: Option<&str>) -> MediaItemUpdate {
  MediaItemUpdate {
    persistent_id: track_key.map(|key| format!("iap2:track:{key}")),
    title: media.title.clone(),
    album: media.album.clone(),
    album_uri: None,
    album_artist: media.album_artist.clone(),
    artist: media.artist.clone(),
    artist_uri: None,
    liked: media.liked,
    artwork_id: None,
    duration_ms: media.duration_ms,
    media_types: None,
    track_number: media.track_number,
    track_count: media.track_count,
    is_like_supported: media.like_supported,
    is_ban_supported: media.ban_supported,
    is_banned: media.banned,
    is_resident_on_device: media.resident_on_device,
    chapter_count: media.chapter_count,
  }
}

fn translate_playback(play: &PlaybackAttributes) -> PlaybackUpdate {
  PlaybackUpdate {
    playing: play.state.map(|s| matches!(s, Iap2PlaybackState::Playing)),
    position_ms: play.position_ms,
    shuffle: play.shuffle_mode.map(|m| m.is_on()),
    shuffle_mode: play.shuffle_mode.map(translate_shuffle),
    repeat: play.repeat.map(translate_repeat),
    app_bundle: play.app_bundle.clone(),
    app_display_name: play.app_display_name.clone(),
    queue_index: play.queue_index,
    queue_count: play.queue_count,
    queue_chapter_index: play.queue_chapter_index,
    playback_speed: play.playback_speed_hundredths.map(|h| f32::from(h) / 100.0),
    set_elapsed_time_available: play.set_elapsed_time_available,
    queue_list_avail: None,
    apple_music_radio_ad: play.apple_music_radio_ad,
    apple_music_radio_station_name: play.apple_music_radio_station_name.clone(),
  }
}

fn translate_repeat(mode: Iap2Repeat) -> RepeatMode {
  match mode {
    Iap2Repeat::Off => RepeatMode::Off,
    Iap2Repeat::Track => RepeatMode::One,
    Iap2Repeat::All => RepeatMode::All,
  }
}

fn translate_shuffle(mode: Iap2Shuffle) -> ShuffleMode {
  match mode {
    Iap2Shuffle::Off => ShuffleMode::Off,
    Iap2Shuffle::Songs => ShuffleMode::Songs,
    Iap2Shuffle::Albums => ShuffleMode::Albums,
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
