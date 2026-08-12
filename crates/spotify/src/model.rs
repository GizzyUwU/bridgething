use std::collections::HashMap;

use librespot_protocol::{
  connect::Cluster,
  devices::DeviceType,
  metadata::{Album as PbAlbum, Artist as PbArtist, Episode as PbEpisode, Show as PbShow, Track as PbTrack},
  player::{PlayerState as PbPlayerState, ProvidedTrack},
};

use crate::util::{gid_to_base62, image_hex};

const DELIMITER_URI: &str = "spotify:delimiter";

#[derive(Debug, Clone, Default)]
pub struct Artist {
  pub uri: String,
  pub name: String,
}

#[derive(Debug, Clone, Default)]
pub struct Album {
  pub uri: String,
  pub name: String,
  pub image_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct Track {
  pub uri: String,
  pub uid: String,
  pub name: String,
  pub artists: Vec<Artist>,
  pub album: Album,
  pub duration_ms: u32,
  pub image_id: String,
  pub is_episode: bool,
  pub saved: bool,
  pub queued: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RepeatMode {
  #[default]
  Off,
  Context,
  Track,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryScope {
  Saved,
  Playlists,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QueuePosition {
  #[default]
  Append,
  Next,
  Index {
    at: u32,
  },
}

#[derive(Debug, Clone, Default)]
pub struct PlayerState {
  pub track: Option<Track>,
  pub context_uri: String,
  pub context_name: String,
  pub is_paused: bool,
  pub position_ms: u32,
  pub duration_ms: u32,
  pub shuffle: bool,
  pub repeat: RepeatMode,
  pub playing_remotely: bool,
  pub remote_device_id: String,
  pub on_remote_speaker: bool,
  pub can_seek: bool,
  pub can_skip_next: bool,
  pub can_skip_prev: bool,
  pub can_toggle_shuffle: bool,
  pub can_repeat_context: bool,
  pub can_repeat_track: bool,
  pub can_set_queue: bool,
  pub can_insert_into_next_tracks: bool,
  pub can_add_to_queue: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Queue {
  pub previous: Vec<Track>,
  pub current: Option<Track>,
  pub next: Vec<Track>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DeviceKind {
  #[default]
  Unknown,
  Phone,
  Tablet,
  Computer,
  Speaker,
  Tv,
  GameConsole,
  Automobile,
  Wearable,
}

#[derive(Debug, Clone, Default)]
pub struct Device {
  pub id: String,
  pub name: String,
  pub kind: DeviceKind,
  pub is_active: bool,
  pub volume: f32,
}

#[derive(Debug, Clone, Default)]
pub struct BrowseItem {
  pub uri: String,
  pub title: String,
  pub subtitle: String,
  pub image_id: String,
  pub artists: Vec<Artist>,
  pub album: Album,
  pub duration_ms: u32,
  pub saved: bool,
  pub playable: bool,
  pub has_children: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Shelf {
  pub id: String,
  pub title: String,
  pub items: Vec<BrowseItem>,
  pub total: u32,
}

#[derive(Debug, Clone, Default)]
pub struct BrowsePage {
  pub items: Vec<BrowseItem>,
  pub total: Option<u32>,
  pub has_more: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ProductState {
  pub product: String,
  pub catalogue: String,
  pub country: String,
  pub is_premium: bool,
  pub can_use_superbird: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SearchResults {
  pub tracks: Vec<BrowseItem>,
  pub albums: Vec<BrowseItem>,
  pub artists: Vec<BrowseItem>,
  pub playlists: Vec<BrowseItem>,
  pub shows: Vec<BrowseItem>,
  pub episodes: Vec<BrowseItem>,
}

#[derive(Debug, Clone)]
pub enum AuthState {
  LoggedOut,
  Pending { url: String, code: String },
  LoggedIn { username: String },
  Failed { reason: String },
}

// ---- cluster -> reduced (the dealer firehose) -------------------------------

fn clamp_ms(v: i64) -> u32 {
  v.clamp(0, u32::MAX as i64) as u32
}

fn artists_from_meta(md: &HashMap<String, String>, fallback_uri: &str) -> Vec<Artist> {
  let mut out = Vec::new();
  if let Some(name) = md.get("artist_name") {
    out.push(Artist {
      name: name.clone(),
      uri: md
        .get("artist_uri")
        .cloned()
        .unwrap_or_else(|| fallback_uri.to_string()),
    });
  }
  let mut i = 1;
  while let Some(name) = md.get(&format!("artist_name:{i}")) {
    out.push(Artist {
      name: name.clone(),
      uri: md.get(&format!("artist_uri:{i}")).cloned().unwrap_or_default(),
    });
    i += 1;
  }
  out
}

pub fn cdn_image_ref(url: &str) -> String {
  for p in ["https://i.scdn.co/image/", "http://i.scdn.co/image/", "spotify:image:"] {
    if let Some(rest) = url.strip_prefix(p) {
      let hex = rest.trim_end_matches('/');
      if !hex.is_empty() && !hex.contains('/') {
        return hex.to_string();
      }
    }
  }
  url.to_string()
}

fn image_from_meta(md: &HashMap<String, String>) -> String {
  for k in ["image_xlarge_url", "image_large_url", "image_url", "image_small_url"] {
    if let Some(v) = md.get(k)
      && !v.is_empty()
    {
      return cdn_image_ref(v);
    }
  }
  String::new()
}

fn active_is_remote_speaker(cluster: &Cluster) -> bool {
  if cluster.active_device_id.is_empty() {
    return false;
  }
  match cluster.device.get(&cluster.active_device_id) {
    Some(info) => !matches!(
      info.device_type.enum_value_or_default(),
      DeviceType::SMARTPHONE | DeviceType::TABLET
    ),
    None => false,
  }
}

pub fn track_from_provided(pt: &ProvidedTrack) -> Track {
  let md = &pt.metadata;
  let is_episode =
    md.get("media.type").map(|s| s == "audio").unwrap_or(false) || pt.uri.starts_with("spotify:episode:");
  let artists = artists_from_meta(md, &pt.artist_uri);
  Track {
    uri: pt.uri.clone(),
    uid: pt.uid.clone(),
    name: md.get("title").cloned().unwrap_or_default(),
    album: Album {
      uri: if pt.album_uri.is_empty() {
        md.get("album_uri").cloned().unwrap_or_default()
      } else {
        pt.album_uri.clone()
      },
      name: md.get("album_title").cloned().unwrap_or_default(),
      image_id: image_from_meta(md),
    },
    artists,
    duration_ms: md.get("duration").and_then(|d| d.parse().ok()).unwrap_or(0),
    image_id: image_from_meta(md),
    is_episode,
    saved: md.get("collection.in_collection").map(|s| s == "true").unwrap_or(false),
    queued: pt.provider == "queue",
  }
}

fn repeat_of(ps: &PbPlayerState) -> RepeatMode {
  let o = &ps.options;
  if o.repeating_track {
    RepeatMode::Track
  } else if o.repeating_context {
    RepeatMode::Context
  } else {
    RepeatMode::Off
  }
}

pub fn position_now(ps: &PbPlayerState) -> u32 {
  let duration_ms = clamp_ms(ps.duration);
  let base = clamp_ms(ps.position_as_of_timestamp);
  if ps.is_playing && !ps.is_paused && ps.timestamp > 0 {
    let elapsed = clamp_ms((crate::util::now_ms() as i64).saturating_sub(ps.timestamp));
    let live = base.saturating_add(elapsed);
    if duration_ms > 0 { live.min(duration_ms) } else { live }
  } else {
    base
  }
}

pub fn player_state(cluster: &Cluster) -> PlayerState {
  let ps = &cluster.player_state;
  let r = &ps.restrictions;
  let track = if ps.track.uri.is_empty() {
    None
  } else {
    Some(track_from_provided(&ps.track))
  };
  let duration_ms = clamp_ms(ps.duration);
  let playing = ps.is_playing && !ps.is_paused;
  let position_ms = position_now(ps);
  PlayerState {
    context_uri: ps.context_uri.clone(),
    context_name: ps
      .context_metadata
      .get("context_description")
      .cloned()
      .unwrap_or_default(),
    is_paused: !playing,
    position_ms,
    duration_ms,
    shuffle: ps.options.shuffling_context,
    repeat: repeat_of(ps),
    playing_remotely: !cluster.active_device_id.is_empty(),
    remote_device_id: cluster.active_device_id.clone(),
    on_remote_speaker: active_is_remote_speaker(cluster),
    can_seek: r.disallow_seeking_reasons.is_empty(),
    can_skip_next: r.disallow_skipping_next_reasons.is_empty(),
    can_skip_prev: r.disallow_skipping_prev_reasons.is_empty(),
    can_toggle_shuffle: r.disallow_toggling_shuffle_reasons.is_empty(),
    can_repeat_context: r.disallow_toggling_repeat_context_reasons.is_empty(),
    can_repeat_track: r.disallow_toggling_repeat_track_reasons.is_empty(),
    can_set_queue: r.disallow_set_queue_reasons.is_empty(),
    can_insert_into_next_tracks: r.disallow_inserting_into_next_tracks_reasons.is_empty(),
    can_add_to_queue: r.disallow_add_to_queue_reasons.is_empty(),
    track,
  }
}

pub fn queue(cluster: &Cluster) -> Queue {
  let ps = &cluster.player_state;
  let content = |pt: &&ProvidedTrack| pt.uri != DELIMITER_URI;
  Queue {
    previous: ps.prev_tracks.iter().filter(content).map(track_from_provided).collect(),
    current: if ps.track.uri.is_empty() {
      None
    } else {
      Some(track_from_provided(&ps.track))
    },
    next: ps.next_tracks.iter().filter(content).map(track_from_provided).collect(),
  }
}

pub fn raw_next_index(next_tracks: &[ProvidedTrack], filtered_index: u32) -> usize {
  let mut seen = 0u32;
  for (raw, track) in next_tracks.iter().enumerate() {
    if track.uri == DELIMITER_URI {
      continue;
    }
    if seen == filtered_index {
      return raw;
    }
    seen += 1;
  }
  next_tracks.len()
}

fn device_kind(kind: DeviceType) -> DeviceKind {
  match kind {
    DeviceType::SMARTPHONE => DeviceKind::Phone,
    DeviceType::TABLET => DeviceKind::Tablet,
    DeviceType::COMPUTER | DeviceType::CHROMEBOOK => DeviceKind::Computer,
    DeviceType::SPEAKER
    | DeviceType::AVR
    | DeviceType::AUDIO_DONGLE
    | DeviceType::CAST_AUDIO
    | DeviceType::HOME_THING => DeviceKind::Speaker,
    DeviceType::TV | DeviceType::STB | DeviceType::CAST_VIDEO => DeviceKind::Tv,
    DeviceType::GAME_CONSOLE => DeviceKind::GameConsole,
    DeviceType::AUTOMOBILE | DeviceType::CAR_THING => DeviceKind::Automobile,
    DeviceType::SMARTWATCH => DeviceKind::Wearable,
    DeviceType::UNKNOWN | DeviceType::UNKNOWN_SPOTIFY | DeviceType::OBSERVER => DeviceKind::Unknown,
  }
}

pub fn devices(cluster: &Cluster, me: &str) -> Vec<Device> {
  let mut out = Vec::new();
  for (id, info) in &cluster.device {
    if id == me {
      continue;
    }
    out.push(Device {
      id: id.clone(),
      name: info.name.clone(),
      kind: device_kind(info.device_type.enum_value_or_default()),
      is_active: *id == cluster.active_device_id,
      volume: if info.volume > 0 {
        info.volume as f32 / 65535.0
      } else {
        0.0
      },
    });
  }
  out
}

pub fn fill_track_from_proto(track: &mut Track, t: &PbTrack) {
  if track.artists.is_empty() {
    track.artists = t
      .artist
      .iter()
      .filter(|a| !a.name().is_empty())
      .map(|a| Artist {
        name: a.name().to_string(),
        uri: if a.gid().is_empty() {
          String::new()
        } else {
          format!("spotify:artist:{}", gid_to_base62(a.gid()))
        },
      })
      .collect();
  }
  if track.duration_ms == 0 {
    track.duration_ms = t.duration().max(0) as u32;
  }
  if track.name.is_empty() {
    track.name = t.name().to_string();
  }
  if track.album.name.is_empty() {
    track.album.name = t.album.name().to_string();
  }
  if track.image_id.is_empty() {
    track.image_id = image_hex(&t.album.cover_group);
  }
}

pub fn fill_track_from_cached(track: &mut Track, cached: &Track) {
  if track.artists.is_empty() {
    track.artists = cached.artists.clone();
  }
  if track.duration_ms == 0 {
    track.duration_ms = cached.duration_ms;
  }
  if track.name.is_empty() {
    track.name = cached.name.clone();
  }
  if track.album.name.is_empty() {
    track.album.name = cached.album.name.clone();
  }
  if track.image_id.is_empty() {
    track.image_id = cached.image_id.clone();
  }
}

// ---- spclient metadata protos -> BrowseItem (per-uri content hydration) -----

fn pb_artists(artists: &[PbArtist]) -> Vec<Artist> {
  artists
    .iter()
    .filter(|a| !a.name().is_empty())
    .map(|a| Artist {
      name: a.name().to_string(),
      uri: if a.gid().is_empty() {
        String::new()
      } else {
        format!("spotify:artist:{}", gid_to_base62(a.gid()))
      },
    })
    .collect()
}

pub fn browse_track(uri: &str, t: &PbTrack) -> BrowseItem {
  let artists = pb_artists(&t.artist);
  BrowseItem {
    uri: uri.to_string(),
    title: t.name().to_string(),
    subtitle: artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", "),
    image_id: image_hex(&t.album.cover_group),
    album: Album {
      uri: if t.album.gid().is_empty() {
        String::new()
      } else {
        format!("spotify:album:{}", gid_to_base62(t.album.gid()))
      },
      name: t.album.name().to_string(),
      image_id: image_hex(&t.album.cover_group),
    },
    artists,
    duration_ms: t.duration().max(0) as u32,
    saved: false,
    playable: true,
    has_children: false,
  }
}

pub fn browse_album(uri: &str, a: &PbAlbum) -> BrowseItem {
  let artists = pb_artists(&a.artist);
  BrowseItem {
    uri: uri.to_string(),
    title: a.name().to_string(),
    subtitle: artists.iter().map(|x| x.name.as_str()).collect::<Vec<_>>().join(", "),
    image_id: image_hex(&a.cover_group),
    artists,
    playable: true,
    has_children: true,
    ..Default::default()
  }
}

pub fn browse_artist(uri: &str, a: &PbArtist) -> BrowseItem {
  BrowseItem {
    uri: uri.to_string(),
    title: a.name().to_string(),
    subtitle: "Artist".to_string(),
    image_id: image_hex(&a.portrait_group),
    playable: true,
    has_children: true,
    ..Default::default()
  }
}

pub fn browse_show(uri: &str, s: &PbShow) -> BrowseItem {
  BrowseItem {
    uri: uri.to_string(),
    title: s.name().to_string(),
    subtitle: s.publisher().to_string(),
    image_id: image_hex(&s.cover_image),
    playable: false,
    has_children: true,
    ..Default::default()
  }
}

pub fn browse_episode(uri: &str, e: &PbEpisode) -> BrowseItem {
  BrowseItem {
    uri: uri.to_string(),
    title: e.name().to_string(),
    subtitle: e.show.name().to_string(),
    image_id: image_hex(&e.cover_image),
    duration_ms: e.duration().max(0) as u32,
    playable: true,
    has_children: false,
    ..Default::default()
  }
}

pub fn browse_playlist(uri: &str, name: &str, image_id: &str) -> BrowseItem {
  BrowseItem {
    uri: uri.to_string(),
    title: name.to_string(),
    subtitle: "Playlist".to_string(),
    image_id: image_id.to_string(),
    playable: true,
    has_children: true,
    ..Default::default()
  }
}

pub fn playlist_image_hex(attrs: &librespot_protocol::playlist4_external::ListAttributes) -> String {
  fn rank(name: &str) -> i32 {
    match name {
      "default" => 3,
      "large" => 2,
      "xlarge" => 1,
      "small" => 0,
      _ => -1,
    }
  }
  let mut best: (&str, i32) = ("", -1);
  for ps in &attrs.picture_size {
    let r = rank(ps.target_name());
    if !ps.url().is_empty() && r > best.1 {
      best = (ps.url(), r);
    }
  }
  if !best.0.is_empty() {
    return cdn_image_ref(best.0);
  }
  if !attrs.picture().is_empty() {
    return hex::encode(attrs.picture());
  }
  String::new()
}

pub fn show_episode_uris(s: &PbShow) -> Vec<String> {
  s.episode
    .iter()
    .filter(|e| !e.gid().is_empty())
    .map(|e| format!("spotify:episode:{}", gid_to_base62(e.gid())))
    .collect()
}

pub fn album_track_uris(a: &PbAlbum) -> Vec<String> {
  let mut uris = Vec::new();
  for disc in &a.disc {
    for tr in &disc.track {
      if !tr.gid().is_empty() {
        uris.push(format!("spotify:track:{}", gid_to_base62(tr.gid())));
      }
    }
  }
  uris
}

pub fn artist_top_track_uris(a: &PbArtist) -> Vec<String> {
  a.top_track
    .first()
    .map(|tt| {
      tt.track
        .iter()
        .filter(|tr| !tr.gid().is_empty())
        .map(|tr| format!("spotify:track:{}", gid_to_base62(tr.gid())))
        .collect()
    })
    .unwrap_or_default()
}

pub fn artist_release_uris(a: &PbArtist, albums_only: bool, depth: usize) -> Vec<String> {
  let singles = if albums_only { [].as_slice() } else { &a.single_group };
  let mut seen = std::collections::HashSet::new();
  [a.album_group.as_slice(), singles]
    .into_iter()
    .flat_map(|group| {
      group
        .iter()
        .flat_map(|g| g.album.iter())
        .filter(|al| !al.gid().is_empty())
        .map(|al| format!("spotify:album:{}", gid_to_base62(al.gid())))
        .take(depth)
        .collect::<Vec<_>>()
    })
    .filter(|u| seen.insert(u.clone()))
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn cdn_image_ref_reduces_iscdn_keeps_foreign() {
    assert_eq!(cdn_image_ref("https://i.scdn.co/image/ab67deadbeef"), "ab67deadbeef");
    assert_eq!(cdn_image_ref("http://i.scdn.co/image/ab67deadbeef"), "ab67deadbeef");
    assert_eq!(cdn_image_ref("spotify:image:ab67deadbeef"), "ab67deadbeef");
    assert_eq!(cdn_image_ref("https://i.scdn.co/image/ab67deadbeef/"), "ab67deadbeef");
    for url in [
      "https://pickasso.spotifycdn.com/ab/en/default.jpg",
      "https://daylist.spotifycdn.com/v1/early-morning_default.jpg",
      "https://blend-playlist-covers.spotifycdn.com/v2/abc.jpg",
      "https://lexicon-assets.spotifycdn.com/dj/cover.jpg",
      "https://seed-mix-image.spotifycdn.com/v6/img.jpg",
    ] {
      assert_eq!(cdn_image_ref(url), url);
    }
    assert_eq!(cdn_image_ref(""), "");
  }

  fn provided(uri: &str, meta: &[(&str, &str)]) -> ProvidedTrack {
    let mut pt = ProvidedTrack::new();
    pt.uri = uri.to_string();
    for (k, v) in meta {
      pt.metadata.insert(k.to_string(), v.to_string());
    }
    pt
  }

  #[test]
  fn queue_drops_delimiter_and_maps_cluster_metadata() {
    let mut cluster = Cluster::new();
    let ps = cluster.player_state.mut_or_insert_default();
    ps.next_tracks
      .push(provided("spotify:track:a", &[("title", "A"), ("duration", "1000")]));
    ps.next_tracks.push(provided("spotify:delimiter", &[]));
    ps.next_tracks.push(provided("spotify:track:b", &[("title", "B")]));

    let q = queue(&cluster);
    assert_eq!(
      q.next.iter().map(|t| t.uri.as_str()).collect::<Vec<_>>(),
      ["spotify:track:a", "spotify:track:b"]
    );
    assert_eq!(q.next[0].name, "A");
    assert_eq!(q.next[0].duration_ms, 1000);
    assert!(q.next.iter().all(|t| t.uri != "spotify:delimiter"));
  }

  fn delimiter() -> ProvidedTrack {
    provided(DELIMITER_URI, &[])
  }

  fn uris(tracks: &[ProvidedTrack]) -> Vec<&str> {
    tracks.iter().map(|t| t.uri.as_str()).collect()
  }

  fn spliced(raw: &[ProvidedTrack], filtered_index: u32) -> Vec<String> {
    let mut next = raw.to_vec();
    next.insert(raw_next_index(raw, filtered_index), provided("spotify:track:x", &[]));
    let mut cluster = Cluster::new();
    cluster.player_state.mut_or_insert_default().next_tracks = next;
    queue(&cluster).next.into_iter().map(|t| t.uri).collect()
  }

  #[test]
  fn raw_next_index_without_delimiters_is_the_identity_until_the_end() {
    let list = [provided("spotify:track:a", &[]), provided("spotify:track:b", &[])];
    assert_eq!(raw_next_index(&list, 0), 0);
    assert_eq!(raw_next_index(&list, 1), 1);
    assert_eq!(raw_next_index(&list, 2), 2, "one past the last track is the tail");
    assert_eq!(raw_next_index(&list, 9), 2, "far past the end clamps to the tail");
    assert_eq!(raw_next_index(&[], 0), 0);
    assert_eq!(raw_next_index(&[], 7), 0);
  }

  #[test]
  fn raw_next_index_skips_delimiters_wherever_they_sit() {
    let leading = [delimiter(), provided("spotify:track:a", &[])];
    assert_eq!(
      raw_next_index(&leading, 0),
      1,
      "a leading delimiter shifts slot 0 right"
    );
    assert_eq!(raw_next_index(&leading, 1), 2);

    let middle = [
      provided("spotify:track:a", &[]),
      delimiter(),
      provided("spotify:track:b", &[]),
    ];
    assert_eq!(raw_next_index(&middle, 0), 0);
    assert_eq!(raw_next_index(&middle, 1), 2, "slot 1 is past the interior delimiter");
    assert_eq!(raw_next_index(&middle, 2), 3);

    let trailing = [provided("spotify:track:a", &[]), delimiter()];
    assert_eq!(raw_next_index(&trailing, 0), 0);
    assert_eq!(raw_next_index(&trailing, 1), 2, "a trailing delimiter is never split");

    let only_delimiters = [delimiter(), delimiter()];
    assert_eq!(raw_next_index(&only_delimiters, 0), 2);
    assert_eq!(raw_next_index(&only_delimiters, 3), 2);
  }

  #[test]
  fn splice_at_raw_next_index_reads_back_at_the_requested_filtered_index() {
    let lists = [
      vec![],
      vec![provided("spotify:track:a", &[])],
      vec![delimiter(), provided("spotify:track:a", &[])],
      vec![
        provided("spotify:track:a", &[]),
        delimiter(),
        provided("spotify:track:b", &[]),
      ],
      vec![provided("spotify:track:a", &[]), delimiter()],
      vec![
        delimiter(),
        provided("spotify:track:a", &[]),
        provided("spotify:track:b", &[]),
        delimiter(),
        provided("spotify:track:c", &[]),
      ],
    ];
    for list in lists {
      let content = list.iter().filter(|t| t.uri != DELIMITER_URI).count() as u32;
      for n in 0..=content {
        let after = spliced(&list, n);
        assert_eq!(
          after.get(n as usize).map(String::as_str),
          Some("spotify:track:x"),
          "insert at filtered {n} of {:?} landed as {after:?}",
          uris(&list)
        );
      }
      let past_end = spliced(&list, content + 5);
      assert_eq!(
        past_end.last().map(String::as_str),
        Some("spotify:track:x"),
        "an index past the end appends to {:?}",
        uris(&list)
      );
    }
  }

  #[test]
  fn player_state_projects_the_queue_write_restrictions() {
    let mut cluster = Cluster::new();
    let ps = cluster.player_state.mut_or_insert_default();
    ps.track.mut_or_insert_default().uri = "spotify:track:a".to_string();
    let open = player_state(&cluster);
    assert!(open.can_set_queue && open.can_insert_into_next_tracks && open.can_add_to_queue);

    let r = cluster
      .player_state
      .mut_or_insert_default()
      .restrictions
      .mut_or_insert_default();
    r.disallow_set_queue_reasons.push("no_set_queue".to_string());
    r.disallow_inserting_into_next_tracks_reasons
      .push("no_insert".to_string());
    r.disallow_add_to_queue_reasons.push("no_add".to_string());
    let closed = player_state(&cluster);
    assert!(!closed.can_set_queue && !closed.can_insert_into_next_tracks && !closed.can_add_to_queue);
  }

  #[test]
  fn position_now_extrapolates_while_playing() {
    let mut ps = PbPlayerState::new();
    ps.is_playing = true;
    ps.is_paused = false;
    ps.position_as_of_timestamp = 1_000;
    ps.duration = 180_000;
    ps.timestamp = (crate::util::now_ms() as i64) - 5_000;
    let pos = position_now(&ps);
    assert!(
      (6_000..=6_500).contains(&pos),
      "playhead extrapolates anchor+elapsed (~6000ms), got {pos}"
    );
  }

  #[test]
  fn position_now_is_frozen_while_paused() {
    let mut ps = PbPlayerState::new();
    ps.is_paused = true;
    ps.position_as_of_timestamp = 42_000;
    ps.timestamp = (crate::util::now_ms() as i64) - 5_000;
    assert_eq!(position_now(&ps), 42_000, "a paused playhead does not advance");
  }

  #[test]
  fn position_now_clamps_to_duration() {
    let mut ps = PbPlayerState::new();
    ps.is_playing = true;
    ps.position_as_of_timestamp = 170_000;
    ps.duration = 180_000;
    ps.timestamp = (crate::util::now_ms() as i64) - 60_000;
    assert_eq!(
      position_now(&ps),
      180_000,
      "extrapolation never runs past the track duration"
    );
  }
}
