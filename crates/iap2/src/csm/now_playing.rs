//! Typed CSMs for the iAP2 NowPlaying surface.
//!
//! Four messages live here:
//!
//! - [`StartNowPlayingUpdates`] (`0x5000`) - accessory to iPhone, sent
//!   once after identification. Carries two subscribe lists (one for
//!   MediaItem attributes, one for Playback attributes); iPhone only
//!   sends back the sub-ids we subscribe to.
//! - [`NowPlayingUpdate`] (`0x5001`) - iPhone to accessory, fired on
//!   any subscribed attribute change. Two outer group params, each a
//!   nested CSM-format block of attribute TLVs.
//! - [`StopNowPlayingUpdates`] (`0x5002`) - accessory to iPhone, sent
//!   on tear-down. Empty body.
//! - [`SetNowPlayingInformation`] (`0x5003`) - accessory to iPhone,
//!   sent in response to a Direct User Action that needs absolute
//!   playback control (scrub thumb, queue-row tap). Three optional
//!   params: ElapsedTime, PlaybackQueueIndex, QueueListContentTransferStartIndex.
//!
//! The subscribe-list encoding looks unusual at first: each subscribed
//! sub-id is its own empty-payload TLV inside the outer group, with
//! the sub-id sitting in the TLV's `param_id` field. iAP2 reuses
//! "presence of empty parameter" as its boolean / flag pattern across
//! the protocol; this is the same pattern applied to a list. See
//! cleanroom doc `protocol/60_now_playing_inner.md`.
//!
//! Inbound updates are deltas: any attribute that changed shows up in
//! a fresh `NowPlayingUpdate`, and the daemon merges into stable
//! state. The decode here populates `Option<T>` for every attribute,
//! so absence and explicit presence stay distinguishable to the
//! merge layer.

use bytes::Bytes;

use super::{Csm, CsmDecodeError, CsmFrame, CsmParam, CsmParamFieldDecode, decode_param_block, encode_param_block};

/// CSMs the accessory sends in this layer. iPhone silently drops any
/// CSM not in this list; identification merges these into param 6 of
/// `IdentificationInformation`.
pub const SENT_BY_ACCESSORY: &[u16] = &[
  StartNowPlayingUpdates::CSM_MSG_ID,
  StopNowPlayingUpdates::CSM_MSG_ID,
  SetNowPlayingInformation::CSM_MSG_ID,
];

/// CSMs the accessory accepts in this layer. Identification merges
/// these into param 7 of `IdentificationInformation`.
pub const RECEIVED_BY_ACCESSORY: &[u16] = &[NowPlayingUpdate::CSM_MSG_ID];

/// MediaItem attribute sub-ids the accessory subscribes to. iPhone
/// only sends these back. See cleanroom `protocol/60_now_playing_inner.md`
/// for the canonical attribute table.
pub const MEDIA_ITEM_SUBSCRIBE: &[u16] = &[
  MEDIA_ITEM_PERSISTENT_ID,
  MEDIA_ITEM_TITLE,
  MEDIA_ITEM_DURATION_MS,
  MEDIA_ITEM_ALBUM,
  MEDIA_ITEM_ALBUM_TRACK_NUMBER,
  MEDIA_ITEM_ALBUM_TRACK_COUNT,
  MEDIA_ITEM_ARTIST,
  MEDIA_ITEM_ALBUM_ARTIST,
  MEDIA_ITEM_LIKED,
  MEDIA_ITEM_ARTWORK_ID,
];

/// Playback attribute sub-ids the accessory subscribes to.
///
/// The `PlaybackQueueList*` trio (`0x0E`/`0x0F`/`0x11`) is intentionally
/// omitted. Those sub-ids require companion subscribe-side fields
/// (`PlaybackQueueListContentTransferInfoRequest`,
/// `PlaybackQueueListContentTransferSize`) that this CSM does not carry,
/// and iOS silently rejects the entire subscribe when they are listed
/// without the companions.
pub const PLAYBACK_SUBSCRIBE: &[u16] = &[
  PLAYBACK_STATE,
  PLAYBACK_POSITION_MS,
  PLAYBACK_QUEUE_INDEX,
  PLAYBACK_QUEUE_COUNT,
  PLAYBACK_SHUFFLE_MODE,
  PLAYBACK_REPEAT,
  PLAYBACK_APP_DISPLAY_NAME,
  PLAYBACK_LIBRARY_UNIQUE_ID,
  PLAYBACK_SPEED,
  PLAYBACK_SET_ELAPSED_TIME_AVAILABLE,
  PLAYBACK_APP_BUNDLE,
];

const MEDIA_ITEM_PERSISTENT_ID: u16 = 0x00;
const MEDIA_ITEM_TITLE: u16 = 0x01;
const MEDIA_ITEM_MEDIA_TYPE: u16 = 0x02;
const MEDIA_ITEM_DURATION_MS: u16 = 0x04;
const MEDIA_ITEM_ALBUM: u16 = 0x06;
const MEDIA_ITEM_ALBUM_TRACK_NUMBER: u16 = 0x07;
const MEDIA_ITEM_ALBUM_TRACK_COUNT: u16 = 0x08;
const MEDIA_ITEM_ARTIST: u16 = 0x0C;
const MEDIA_ITEM_ALBUM_ARTIST: u16 = 0x0E;
const MEDIA_ITEM_LIKE_SUPPORTED: u16 = 0x15;
const MEDIA_ITEM_BAN_SUPPORTED: u16 = 0x16;
const MEDIA_ITEM_LIKED: u16 = 0x17;
const MEDIA_ITEM_BANNED: u16 = 0x18;
const MEDIA_ITEM_RESIDENT_ON_DEVICE: u16 = 0x19;
const MEDIA_ITEM_ARTWORK_ID: u16 = 0x1A;
const MEDIA_ITEM_CHAPTER_COUNT: u16 = 0x1B;

const PLAYBACK_STATE: u16 = 0x00;
const PLAYBACK_POSITION_MS: u16 = 0x01;
const PLAYBACK_QUEUE_INDEX: u16 = 0x02;
const PLAYBACK_QUEUE_COUNT: u16 = 0x03;
const PLAYBACK_QUEUE_CHAPTER_INDEX: u16 = 0x04;
const PLAYBACK_SHUFFLE_MODE: u16 = 0x05;
const PLAYBACK_REPEAT: u16 = 0x06;
const PLAYBACK_APP_DISPLAY_NAME: u16 = 0x07;
const PLAYBACK_LIBRARY_UNIQUE_ID: u16 = 0x08;
const PLAYBACK_RADIO_AD: u16 = 0x09;
const PLAYBACK_RADIO_STATION_NAME: u16 = 0x0A;
const PLAYBACK_SPEED: u16 = 0x0C;
const PLAYBACK_SET_ELAPSED_TIME_AVAILABLE: u16 = 0x0D;
const PLAYBACK_QUEUE_LIST_AVAIL: u16 = 0x0E;
const PLAYBACK_QUEUE_LIST_TRANSFER_ID: u16 = 0x0F;
const PLAYBACK_APP_BUNDLE: u16 = 0x10;
const PLAYBACK_QUEUE_LIST_CONTENT_TRANSFER: u16 = 0x11;

const NOW_PLAYING_PARAM_MEDIA_ITEM: u16 = 0;
const NOW_PLAYING_PARAM_PLAYBACK: u16 = 1;

/// `0x5000` accessory -> iPhone. Subscribes to the listed attribute
/// ids; iPhone will only push back attributes whose sub-id appears
/// here. Construct with [`StartNowPlayingUpdates::standard`] for the
/// canonical Bridgething subscription set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartNowPlayingUpdates {
  pub media_item: Vec<u16>,
  pub playback: Vec<u16>,
}

impl StartNowPlayingUpdates {
  pub const CSM_MSG_ID: u16 = 0x5000;

  pub fn standard() -> Self {
    Self {
      media_item: MEDIA_ITEM_SUBSCRIBE.to_vec(),
      playback: PLAYBACK_SUBSCRIBE.to_vec(),
    }
  }
}

impl From<StartNowPlayingUpdates> for CsmFrame {
  fn from(value: StartNowPlayingUpdates) -> Self {
    let params: Vec<CsmParam> = vec![
      CsmParam {
        id: NOW_PLAYING_PARAM_MEDIA_ITEM,
        payload: encode_subscribe_list(&value.media_item),
      },
      CsmParam {
        id: NOW_PLAYING_PARAM_PLAYBACK,
        payload: encode_subscribe_list(&value.playback),
      },
    ];

    Self {
      msg_id: StartNowPlayingUpdates::CSM_MSG_ID,
      params,
    }
  }
}

/// `0x5002` accessory -> iPhone. Empty body; tells iPhone to stop
/// pushing `NowPlayingUpdate`s.
#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0x5002)]
pub struct StopNowPlayingUpdates;

/// `0x5003` accessory -> iPhone. The only iAP2 way to seek absolutely
/// or jump to a specific queue index. Must be sent only in response to
/// a Direct User Action; cleanroom doc 80 spells out the gating.
///
/// `elapsed_time_ms` is only honored when the most recent
/// `NowPlayingUpdate.PlaybackSetElapsedTimeAvailable` was true; sending
/// while false is silently ignored or rejected.
///
/// `queue_index` jumps the playback head to a specific queue position
/// (0-based).
///
/// `queue_list_content_transfer_start_index` is the window-into-queue
/// start for the file-transferred queue listing; `0xFFFF_FFFF` lets iOS
/// center on the current track.
#[derive(Csm, Debug, Clone, Default, PartialEq, Eq)]
#[csm(id = 0x5003)]
pub struct SetNowPlayingInformation {
  #[csm(param = 0)]
  pub elapsed_time_ms: Option<u32>,
  #[csm(param = 1)]
  pub queue_index: Option<u32>,
  #[csm(param = 2)]
  pub queue_list_content_transfer_start_index: Option<u32>,
}

/// `0x5001` iPhone -> accessory. Delta-shaped: any attribute the
/// iPhone has fresh information about appears in the matching group;
/// absent attributes mean "nothing to say about this field," not
/// "field cleared." The session merges deltas into stable state; this
/// type is just the wire-decode result.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NowPlayingUpdate {
  pub media_item: Option<MediaItemAttributes>,
  pub playback: Option<PlaybackAttributes>,
}

impl NowPlayingUpdate {
  pub const CSM_MSG_ID: u16 = 0x5001;
}

impl TryFrom<CsmFrame> for NowPlayingUpdate {
  type Error = CsmDecodeError;

  fn try_from(frame: CsmFrame) -> Result<Self, Self::Error> {
    if frame.msg_id != Self::CSM_MSG_ID {
      return Err(CsmDecodeError::WrongMsgId {
        got: frame.msg_id,
        expected: Self::CSM_MSG_ID,
      });
    }
    let mut params = frame.params;
    let media_item = optional_group::<MediaItemAttributes>(NOW_PLAYING_PARAM_MEDIA_ITEM, &mut params)?;
    let playback = optional_group::<PlaybackAttributes>(NOW_PLAYING_PARAM_PLAYBACK, &mut params)?;
    Ok(Self { media_item, playback })
  }
}

/// MediaType is multi-typed: an item can carry more than one bit. iAP2
/// encodes it as a u32 BE with each value contributing a distinct bit
/// position; we expand it into a `Vec<MediaTypeKind>` for ergonomics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaTypeKind {
  Music,
  Podcast,
  AudioBook,
}

/// Per-track attributes carried inside the `NowPlayingUpdate` param-0
/// group. All fields optional; iPhone sends only the ones that
/// changed since the last update.
///
/// `artist` is the track-credited artist (sub-id 0x0C).
/// `album_artist` (sub-id 0x0E) is the album-level credited artist and
/// is semantically distinct.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MediaItemAttributes {
  pub persistent_id: Option<u64>,
  pub title: Option<String>,
  pub media_types: Option<Vec<MediaTypeKind>>,
  pub duration_ms: Option<u32>,
  pub album: Option<String>,
  pub track_number: Option<u16>,
  pub track_count: Option<u16>,
  pub artist: Option<String>,
  pub album_artist: Option<String>,
  pub like_supported: Option<bool>,
  pub ban_supported: Option<bool>,
  pub liked: Option<bool>,
  pub banned: Option<bool>,
  pub resident_on_device: Option<bool>,
  pub artwork_id: Option<u8>,
  pub chapter_count: Option<u16>,
}

impl MediaItemAttributes {
  /// Decode a single MediaItem parameter group from its inner payload
  /// (the bytes inside a `param 0` of a `NowPlayingUpdate`, or one
  /// element of a queue snapshot blob). The payload is the inner
  /// CSM-format param block; the outer 4-byte param header is the
  /// caller's responsibility.
  pub fn decode_group(payload: Bytes) -> Result<Self, CsmDecodeError> {
    Self::decode_group_inner(payload)
  }

  fn decode_group_inner(payload: Bytes) -> Result<Self, CsmDecodeError> {
    let mut params = decode_param_block(payload)?;
    let persistent_id = take_optional_be_u64(MEDIA_ITEM_PERSISTENT_ID, &mut params)?;
    let title = Option::<String>::decode_field(MEDIA_ITEM_TITLE, &mut params)?;
    let media_types = take_optional_be_u32(MEDIA_ITEM_MEDIA_TYPE, &mut params)?.map(decode_media_types);
    let duration_ms = take_optional_be_u32(MEDIA_ITEM_DURATION_MS, &mut params)?;
    let album = Option::<String>::decode_field(MEDIA_ITEM_ALBUM, &mut params)?;
    let track_number = take_optional_be_u16(MEDIA_ITEM_ALBUM_TRACK_NUMBER, &mut params)?;
    let track_count = take_optional_be_u16(MEDIA_ITEM_ALBUM_TRACK_COUNT, &mut params)?;
    let artist = Option::<String>::decode_field(MEDIA_ITEM_ARTIST, &mut params)?;
    let album_artist = Option::<String>::decode_field(MEDIA_ITEM_ALBUM_ARTIST, &mut params)?;
    let like_supported = take_optional_presence_bool(MEDIA_ITEM_LIKE_SUPPORTED, &mut params)?;
    let ban_supported = take_optional_presence_bool(MEDIA_ITEM_BAN_SUPPORTED, &mut params)?;
    let liked = take_optional_presence_bool(MEDIA_ITEM_LIKED, &mut params)?;
    let banned = take_optional_presence_bool(MEDIA_ITEM_BANNED, &mut params)?;
    let resident_on_device = take_optional_presence_bool(MEDIA_ITEM_RESIDENT_ON_DEVICE, &mut params)?;
    let artwork_id = take_optional_be_u8(MEDIA_ITEM_ARTWORK_ID, &mut params)?;
    let chapter_count = take_optional_be_u16(MEDIA_ITEM_CHAPTER_COUNT, &mut params)?;
    Ok(Self {
      persistent_id,
      title,
      media_types,
      duration_ms,
      album,
      track_number,
      track_count,
      artist,
      album_artist,
      like_supported,
      ban_supported,
      liked,
      banned,
      resident_on_device,
      artwork_id,
      chapter_count,
    })
  }
}

/// Per-playback-session attributes carried inside the
/// `NowPlayingUpdate` param-1 group. `app_bundle` is the iOS app's
/// bundle identifier (e.g. `"com.spotify.client"`) - the most
/// reliable signal for "what audio app is foregrounded right now."
///
/// `set_elapsed_time_available` is the load-bearing scrub gate; webapp
/// scrub UI must be disabled when this is false.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlaybackAttributes {
  pub state: Option<PlaybackState>,
  pub position_ms: Option<u32>,
  pub queue_index: Option<u32>,
  pub queue_count: Option<u32>,
  pub queue_chapter_index: Option<u32>,
  pub shuffle_mode: Option<ShuffleMode>,
  pub repeat: Option<RepeatMode>,
  pub app_display_name: Option<String>,
  pub library_unique_id: Option<String>,
  pub apple_music_radio_ad: Option<bool>,
  pub apple_music_radio_station_name: Option<String>,
  pub playback_speed_hundredths: Option<u16>,
  pub set_elapsed_time_available: Option<bool>,
  pub queue_list_avail: Option<bool>,
  pub queue_list_transfer_id: Option<u8>,
  pub app_bundle: Option<String>,
  pub queue_list_content_transfer: Option<()>,
}

impl PlaybackAttributes {
  fn decode_group_inner(payload: Bytes) -> Result<Self, CsmDecodeError> {
    let mut params = decode_param_block(payload)?;
    let state = take_optional_be_u8(PLAYBACK_STATE, &mut params)?.map(PlaybackState::from_byte);
    let position_ms = take_optional_be_u32(PLAYBACK_POSITION_MS, &mut params)?;
    let queue_index = take_optional_be_u32(PLAYBACK_QUEUE_INDEX, &mut params)?;
    let queue_count = take_optional_be_u32(PLAYBACK_QUEUE_COUNT, &mut params)?;
    let queue_chapter_index = take_optional_be_u32(PLAYBACK_QUEUE_CHAPTER_INDEX, &mut params)?;
    let shuffle_mode = take_optional_be_u8(PLAYBACK_SHUFFLE_MODE, &mut params)?.map(ShuffleMode::from_byte);
    let repeat = take_optional_be_u8(PLAYBACK_REPEAT, &mut params)?.map(RepeatMode::from_byte);
    let app_display_name = Option::<String>::decode_field(PLAYBACK_APP_DISPLAY_NAME, &mut params)?;
    let library_unique_id = Option::<String>::decode_field(PLAYBACK_LIBRARY_UNIQUE_ID, &mut params)?;
    let apple_music_radio_ad = take_optional_presence_bool(PLAYBACK_RADIO_AD, &mut params)?;
    let apple_music_radio_station_name = Option::<String>::decode_field(PLAYBACK_RADIO_STATION_NAME, &mut params)?;
    let playback_speed_hundredths = take_optional_be_u16(PLAYBACK_SPEED, &mut params)?;
    let set_elapsed_time_available = take_optional_presence_bool(PLAYBACK_SET_ELAPSED_TIME_AVAILABLE, &mut params)?;
    let queue_list_avail = take_optional_presence_bool(PLAYBACK_QUEUE_LIST_AVAIL, &mut params)?;
    let queue_list_transfer_id = take_optional_be_u8(PLAYBACK_QUEUE_LIST_TRANSFER_ID, &mut params)?;
    let app_bundle = Option::<String>::decode_field(PLAYBACK_APP_BUNDLE, &mut params)?;
    let queue_list_content_transfer = take_optional_presence_marker(PLAYBACK_QUEUE_LIST_CONTENT_TRANSFER, &mut params)?;
    Ok(Self {
      state,
      position_ms,
      queue_index,
      queue_count,
      queue_chapter_index,
      shuffle_mode,
      repeat,
      app_display_name,
      library_unique_id,
      apple_music_radio_ad,
      apple_music_radio_station_name,
      playback_speed_hundredths,
      set_elapsed_time_available,
      queue_list_avail,
      queue_list_transfer_id,
      app_bundle,
      queue_list_content_transfer,
    })
  }
}

/// Decode the queue snapshot blob delivered over File Transfer Session
/// id 2. Each element is a MediaItem parameter group wrapped as one
/// CsmParam entry; the entry's id position carries no information for
/// us (Apple uses it as a slot tag we ignore). Per-element decode is
/// best-effort: malformed entries are dropped with a warn rather than
/// aborting the whole parse — a partially-decoded queue is more useful
/// than no queue.
pub fn decode_queue_snapshot(blob: Bytes) -> Result<Vec<MediaItemAttributes>, CsmDecodeError> {
  let entries = decode_param_block(blob)?;
  let mut out = Vec::with_capacity(entries.len());
  for (idx, CsmParam { id, payload }) in entries.into_iter().enumerate() {
    match MediaItemAttributes::decode_group(payload) {
      Ok(item) => out.push(item),
      Err(err) => {
        tracing::warn!(?err, idx, slot_id = id, "queue snapshot entry decode failed; skipping");
      }
    }
  }
  Ok(out)
}

/// Three-state playback per cleanroom doc 60 catalogue: 0 stopped,
/// 1 playing, 2 paused, 3 seek-forward, 4 seek-backward. We collapse
/// the seeking states into Playing (the elapsed-time keeps changing
/// either way).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
  Stopped,
  Playing,
  Paused,
}

impl PlaybackState {
  fn from_byte(byte: u8) -> Self {
    match byte {
      0 => Self::Stopped,
      1 | 3 | 4 => Self::Playing,
      _ => Self::Paused,
    }
  }
}

/// Apple's repeat-mode convention: 0 off, 1 single track, 2 the whole
/// context. Anything else lands in `Off` defensively.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatMode {
  Off,
  Track,
  All,
}

impl RepeatMode {
  fn from_byte(byte: u8) -> Self {
    match byte {
      1 => Self::Track,
      2 => Self::All,
      _ => Self::Off,
    }
  }

  pub fn as_u32(self) -> u32 {
    match self {
      Self::Off => 0,
      Self::Track => 1,
      Self::All => 2,
    }
  }
}

/// Apple's shuffle-mode convention: 0 off, 1 songs (per-track), 2 albums.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShuffleMode {
  Off,
  Songs,
  Albums,
}

impl ShuffleMode {
  fn from_byte(byte: u8) -> Self {
    match byte {
      1 => Self::Songs,
      2 => Self::Albums,
      _ => Self::Off,
    }
  }

  pub fn is_on(self) -> bool {
    !matches!(self, Self::Off)
  }
}

fn decode_media_types(bits: u32) -> Vec<MediaTypeKind> {
  // iAP2 packs media types as a bitfield. Bit positions match the values
  // in cleanroom doc 60: 0=Music, 1=Podcast, 2=AudioBook (a track may be
  // tagged with multiple at once).
  let mut out = Vec::new();
  if bits & (1 << 0) != 0 {
    out.push(MediaTypeKind::Music);
  }
  if bits & (1 << 1) != 0 {
    out.push(MediaTypeKind::Podcast);
  }
  if bits & (1 << 2) != 0 {
    out.push(MediaTypeKind::AudioBook);
  }
  out
}

fn encode_subscribe_list(ids: &[u16]) -> Bytes {
  let params: Vec<CsmParam> = ids
    .iter()
    .map(|id| CsmParam {
      id: *id,
      payload: Bytes::new(),
    })
    .collect();
  encode_param_block(params)
}

fn optional_group<T>(param_id: u16, params: &mut Vec<CsmParam>) -> Result<Option<T>, CsmDecodeError>
where
  T: GroupDecode,
{
  let pos = match params.iter().position(|p| p.id == param_id) {
    Some(pos) => pos,
    None => return Ok(None),
  };
  let CsmParam { payload, .. } = params.remove(pos);
  if params.iter().any(|p| p.id == param_id) {
    return Err(CsmDecodeError::DuplicateParam(param_id));
  }
  if payload.is_empty() {
    return Ok(Some(T::default_group()));
  }
  Ok(Some(T::decode_group(payload)?))
}

trait GroupDecode: Sized {
  fn decode_group(payload: Bytes) -> Result<Self, CsmDecodeError>;
  fn default_group() -> Self;
}

impl GroupDecode for MediaItemAttributes {
  fn decode_group(payload: Bytes) -> Result<Self, CsmDecodeError> {
    Self::decode_group_inner(payload)
  }
  fn default_group() -> Self {
    Self::default()
  }
}

impl GroupDecode for PlaybackAttributes {
  fn decode_group(payload: Bytes) -> Result<Self, CsmDecodeError> {
    Self::decode_group_inner(payload)
  }
  fn default_group() -> Self {
    Self::default()
  }
}

fn take_optional(param_id: u16, params: &mut Vec<CsmParam>) -> Result<Option<Bytes>, CsmDecodeError> {
  let pos = match params.iter().position(|p| p.id == param_id) {
    Some(p) => p,
    None => return Ok(None),
  };
  let CsmParam { payload, .. } = params.remove(pos);
  if params.iter().any(|p| p.id == param_id) {
    return Err(CsmDecodeError::DuplicateParam(param_id));
  }
  Ok(Some(payload))
}

fn take_optional_be_u8(param_id: u16, params: &mut Vec<CsmParam>) -> Result<Option<u8>, CsmDecodeError> {
  let Some(payload) = take_optional(param_id, params)? else {
    return Ok(None);
  };
  if payload.len() != 1 {
    return Err(CsmDecodeError::ParamLength {
      param_id,
      expected: 1,
      got: payload.len(),
    });
  }
  Ok(Some(payload[0]))
}

fn take_optional_be_u16(param_id: u16, params: &mut Vec<CsmParam>) -> Result<Option<u16>, CsmDecodeError> {
  let Some(payload) = take_optional(param_id, params)? else {
    return Ok(None);
  };
  if payload.len() != 2 {
    return Err(CsmDecodeError::ParamLength {
      param_id,
      expected: 2,
      got: payload.len(),
    });
  }
  Ok(Some(u16::from_be_bytes([payload[0], payload[1]])))
}

fn take_optional_be_u32(param_id: u16, params: &mut Vec<CsmParam>) -> Result<Option<u32>, CsmDecodeError> {
  let Some(payload) = take_optional(param_id, params)? else {
    return Ok(None);
  };
  if payload.len() != 4 {
    return Err(CsmDecodeError::ParamLength {
      param_id,
      expected: 4,
      got: payload.len(),
    });
  }
  Ok(Some(u32::from_be_bytes([
    payload[0], payload[1], payload[2], payload[3],
  ])))
}

fn take_optional_be_u64(param_id: u16, params: &mut Vec<CsmParam>) -> Result<Option<u64>, CsmDecodeError> {
  let Some(payload) = take_optional(param_id, params)? else {
    return Ok(None);
  };
  if payload.len() != 8 {
    return Err(CsmDecodeError::ParamLength {
      param_id,
      expected: 8,
      got: payload.len(),
    });
  }
  let mut buf = [0u8; 8];
  buf.copy_from_slice(&payload);
  Ok(Some(u64::from_be_bytes(buf)))
}

/// Presence-only marker: empty payload = present, omission = absent.
/// Different from `take_optional_presence_bool` because the marker
/// carries no truth value beyond its presence — an explicit `()` rather
/// than a `bool`.
fn take_optional_presence_marker(param_id: u16, params: &mut Vec<CsmParam>) -> Result<Option<()>, CsmDecodeError> {
  let Some(payload) = take_optional(param_id, params)? else {
    return Ok(None);
  };
  if !payload.is_empty() {
    return Err(CsmDecodeError::ParamLength {
      param_id,
      expected: 0,
      got: payload.len(),
    });
  }
  Ok(Some(()))
}

/// "Liked" / "shuffle" arrive as a presence-only marker (empty
/// payload = true, omission = unknown) on iAP2's wire. This matches
/// iAP2's broader convention of using empty-payload TLVs as flags.
fn take_optional_presence_bool(param_id: u16, params: &mut Vec<CsmParam>) -> Result<Option<bool>, CsmDecodeError> {
  let Some(payload) = take_optional(param_id, params)? else {
    return Ok(None);
  };
  if payload.is_empty() {
    Ok(Some(true))
  } else {
    Ok(Some(payload[0] != 0))
  }
}

#[cfg(test)]
mod tests {
  use bytes::{BufMut, BytesMut};
  use tokio_util::codec::{Decoder, Encoder};

  use super::*;
  use crate::csm::{CSM_OUTER_HEADER_LEN, CSM_PARAM_HEADER_LEN, CsmCodec};

  fn build_group(sub_params: &[(u16, &[u8])]) -> Bytes {
    let params: Vec<CsmParam> = sub_params
      .iter()
      .map(|(id, payload)| CsmParam {
        id: *id,
        payload: Bytes::copy_from_slice(payload),
      })
      .collect();
    encode_param_block(params)
  }

  fn frame_with_groups(media: Option<Bytes>, playback: Option<Bytes>) -> CsmFrame {
    let mut params = Vec::with_capacity(2);
    if let Some(m) = media {
      params.push(CsmParam {
        id: NOW_PLAYING_PARAM_MEDIA_ITEM,
        payload: m,
      });
    }
    if let Some(p) = playback {
      params.push(CsmParam {
        id: NOW_PLAYING_PARAM_PLAYBACK,
        payload: p,
      });
    }
    CsmFrame {
      msg_id: NowPlayingUpdate::CSM_MSG_ID,
      params,
    }
  }

  #[test]
  fn start_now_playing_emits_subscribe_list_per_param() {
    let csm = StartNowPlayingUpdates::standard();
    let frame: CsmFrame = csm.into();
    assert_eq!(frame.msg_id, 0x5000);
    let media = frame.find(NOW_PLAYING_PARAM_MEDIA_ITEM).unwrap();
    let media_subs = decode_param_block(media.payload.clone()).unwrap();
    assert_eq!(media_subs.len(), MEDIA_ITEM_SUBSCRIBE.len());
    for (decoded, expected) in media_subs.iter().zip(MEDIA_ITEM_SUBSCRIBE.iter()) {
      assert_eq!(decoded.id, *expected);
      assert!(decoded.payload.is_empty());
    }
    let play = frame.find(NOW_PLAYING_PARAM_PLAYBACK).unwrap();
    let play_subs = decode_param_block(play.payload.clone()).unwrap();
    assert_eq!(play_subs.len(), PLAYBACK_SUBSCRIBE.len());
  }

  #[test]
  fn stop_now_playing_is_empty_csm() {
    let frame: CsmFrame = StopNowPlayingUpdates.into();
    assert_eq!(frame.msg_id, 0x5002);
    assert!(frame.params.is_empty());
    let back: StopNowPlayingUpdates = frame.try_into().unwrap();
    assert_eq!(back, StopNowPlayingUpdates);
  }

  #[test]
  fn set_now_playing_information_round_trips_with_only_elapsed() {
    let csm = SetNowPlayingInformation {
      elapsed_time_ms: Some(123_456),
      queue_index: None,
      queue_list_content_transfer_start_index: None,
    };
    let frame: CsmFrame = csm.clone().into();
    assert_eq!(frame.msg_id, 0x5003);
    // Only param 0 should be present.
    let ids: Vec<u16> = frame.params.iter().map(|p| p.id).collect();
    assert_eq!(ids, vec![0]);
    let decoded: SetNowPlayingInformation = frame.try_into().unwrap();
    assert_eq!(decoded, csm);
  }

  #[test]
  fn set_now_playing_information_round_trips_with_queue_index() {
    let csm = SetNowPlayingInformation {
      elapsed_time_ms: None,
      queue_index: Some(7),
      queue_list_content_transfer_start_index: Some(0xFFFF_FFFF),
    };
    let frame: CsmFrame = csm.clone().into();
    let decoded: SetNowPlayingInformation = frame.try_into().unwrap();
    assert_eq!(decoded, csm);
  }

  #[test]
  fn now_playing_decodes_full_media_and_playback() {
    let media_payload = build_group(&[
      (MEDIA_ITEM_PERSISTENT_ID, &0x0011_2233_4455_6677u64.to_be_bytes()),
      (MEDIA_ITEM_TITLE, b"Hello\0"),
      (MEDIA_ITEM_DURATION_MS, &200_000u32.to_be_bytes()),
      (MEDIA_ITEM_ALBUM, b"World\0"),
      (MEDIA_ITEM_ARTIST, b"Artist\0"),
      (MEDIA_ITEM_ALBUM_ARTIST, b"AlbumArtist\0"),
      (MEDIA_ITEM_LIKED, &[]),
      (MEDIA_ITEM_ARTWORK_ID, &[7]),
    ]);
    let playback_payload = build_group(&[
      (PLAYBACK_STATE, &[1]),
      (PLAYBACK_POSITION_MS, &12_345u32.to_be_bytes()),
      (PLAYBACK_QUEUE_INDEX, &0u32.to_be_bytes()),
      (PLAYBACK_QUEUE_COUNT, &10u32.to_be_bytes()),
      (PLAYBACK_SHUFFLE_MODE, &[1]),
      (PLAYBACK_REPEAT, &[2]),
      (PLAYBACK_APP_DISPLAY_NAME, b"Spotify\0"),
      (PLAYBACK_SET_ELAPSED_TIME_AVAILABLE, &[]),
      (PLAYBACK_APP_BUNDLE, b"com.spotify.client\0"),
    ]);
    let frame = frame_with_groups(Some(media_payload), Some(playback_payload));
    let update: NowPlayingUpdate = frame.try_into().unwrap();
    let media = update.media_item.expect("media_item");
    assert_eq!(media.persistent_id, Some(0x0011_2233_4455_6677u64));
    assert_eq!(media.title.as_deref(), Some("Hello"));
    assert_eq!(media.duration_ms, Some(200_000));
    assert_eq!(media.album.as_deref(), Some("World"));
    assert_eq!(media.artist.as_deref(), Some("Artist"));
    assert_eq!(media.album_artist.as_deref(), Some("AlbumArtist"));
    assert_eq!(media.liked, Some(true));
    assert_eq!(media.artwork_id, Some(7));
    let play = update.playback.expect("playback");
    assert_eq!(play.state, Some(PlaybackState::Playing));
    assert_eq!(play.position_ms, Some(12_345));
    assert_eq!(play.queue_index, Some(0));
    assert_eq!(play.queue_count, Some(10));
    assert_eq!(play.shuffle_mode, Some(ShuffleMode::Songs));
    assert_eq!(play.repeat, Some(RepeatMode::All));
    assert_eq!(play.app_display_name.as_deref(), Some("Spotify"));
    assert_eq!(play.set_elapsed_time_available, Some(true));
    assert_eq!(play.app_bundle.as_deref(), Some("com.spotify.client"));
  }

  #[test]
  fn artist_and_album_artist_are_separately_decoded() {
    let media_payload = build_group(&[
      (MEDIA_ITEM_ARTIST, b"TrackArtist\0"),
      (MEDIA_ITEM_ALBUM_ARTIST, b"AlbumArtist\0"),
    ]);
    let frame = frame_with_groups(Some(media_payload), None);
    let update: NowPlayingUpdate = frame.try_into().unwrap();
    let media = update.media_item.expect("media_item");
    assert_eq!(media.artist.as_deref(), Some("TrackArtist"));
    assert_eq!(media.album_artist.as_deref(), Some("AlbumArtist"));
  }

  #[test]
  fn empty_groups_decode_to_default_attributes() {
    let frame = frame_with_groups(Some(Bytes::new()), Some(Bytes::new()));
    let update: NowPlayingUpdate = frame.try_into().unwrap();
    let media = update.media_item.expect("media_item present");
    assert_eq!(media, MediaItemAttributes::default());
    let play = update.playback.expect("playback present");
    assert_eq!(play, PlaybackAttributes::default());
  }

  #[test]
  fn missing_groups_decode_to_none() {
    let frame = frame_with_groups(None, None);
    let update: NowPlayingUpdate = frame.try_into().unwrap();
    assert!(update.media_item.is_none());
    assert!(update.playback.is_none());
  }

  #[test]
  fn unknown_state_byte_treated_as_paused() {
    let payload = build_group(&[(PLAYBACK_STATE, &[0x05])]);
    let frame = frame_with_groups(None, Some(payload));
    let update: NowPlayingUpdate = frame.try_into().unwrap();
    assert_eq!(update.playback.unwrap().state, Some(PlaybackState::Paused));
  }

  #[test]
  fn seek_states_are_treated_as_playing() {
    for byte in [3u8, 4u8] {
      let payload = build_group(&[(PLAYBACK_STATE, &[byte])]);
      let frame = frame_with_groups(None, Some(payload));
      let update: NowPlayingUpdate = frame.try_into().unwrap();
      assert_eq!(update.playback.unwrap().state, Some(PlaybackState::Playing));
    }
  }

  #[test]
  fn wrong_msg_id_rejects() {
    let frame = CsmFrame::empty(0xAA00);
    let err = NowPlayingUpdate::try_from(frame).unwrap_err();
    assert!(matches!(
      err,
      CsmDecodeError::WrongMsgId {
        got: 0xAA00,
        expected: 0x5001,
      }
    ));
  }

  #[test]
  fn now_playing_update_round_trips_through_codec() {
    let media_payload = build_group(&[(MEDIA_ITEM_TITLE, b"Round\0"), (MEDIA_ITEM_ARTWORK_ID, &[3])]);
    let playback_payload = build_group(&[(PLAYBACK_STATE, &[0]), (PLAYBACK_POSITION_MS, &0u32.to_be_bytes())]);
    let frame = frame_with_groups(Some(media_payload.clone()), Some(playback_payload.clone()));

    let mut buf = BytesMut::new();
    CsmCodec.encode(frame.clone(), &mut buf).unwrap();

    let total = u16::from_be_bytes([buf[2], buf[3]]) as usize;
    assert_eq!(total, buf.len());
    assert!(total > CSM_OUTER_HEADER_LEN + 2 * CSM_PARAM_HEADER_LEN);

    let decoded = CsmCodec.decode(&mut buf).unwrap().unwrap();
    let update: NowPlayingUpdate = decoded.try_into().unwrap();
    let media = update.media_item.expect("media_item");
    assert_eq!(media.title.as_deref(), Some("Round"));
    assert_eq!(media.artwork_id, Some(3));
    let play = update.playback.expect("playback");
    assert_eq!(play.state, Some(PlaybackState::Stopped));
    assert_eq!(play.position_ms, Some(0));
  }

  #[test]
  fn subscribe_list_uses_empty_payload_per_id() {
    let csm = StartNowPlayingUpdates {
      media_item: vec![0x01, 0x06],
      playback: vec![],
    };
    let frame: CsmFrame = csm.into();
    let media = frame.find(NOW_PLAYING_PARAM_MEDIA_ITEM).unwrap();
    let mut payload = BytesMut::new();
    payload.put_slice(&media.payload);
    let mut expected = BytesMut::new();
    expected.put_u16(CSM_PARAM_HEADER_LEN as u16);
    expected.put_u16(0x01);
    expected.put_u16(CSM_PARAM_HEADER_LEN as u16);
    expected.put_u16(0x06);
    assert_eq!(&payload[..], &expected[..]);
  }

  #[test]
  fn media_types_bitfield_decodes_to_combination() {
    let payload = build_group(&[(MEDIA_ITEM_MEDIA_TYPE, &(0b101u32).to_be_bytes())]);
    let frame = frame_with_groups(Some(payload), None);
    let update: NowPlayingUpdate = frame.try_into().unwrap();
    let media = update.media_item.expect("media_item");
    assert_eq!(
      media.media_types.as_deref(),
      Some(&[MediaTypeKind::Music, MediaTypeKind::AudioBook][..])
    );
  }

  #[test]
  fn queue_snapshot_decodes_each_wrapped_media_item() {
    let item_a = build_group(&[
      (MEDIA_ITEM_PERSISTENT_ID, &0x0011_2233_4455_6677u64.to_be_bytes()),
      (MEDIA_ITEM_TITLE, b"A\0"),
      (MEDIA_ITEM_ARTIST, b"AArt\0"),
      (MEDIA_ITEM_ARTWORK_ID, &[1]),
    ]);
    let item_b = build_group(&[
      (MEDIA_ITEM_PERSISTENT_ID, &0x8899_aabb_ccdd_eeffu64.to_be_bytes()),
      (MEDIA_ITEM_TITLE, b"B\0"),
      (MEDIA_ITEM_DURATION_MS, &123_000u32.to_be_bytes()),
    ]);
    let blob = encode_param_block(vec![
      CsmParam { id: 0, payload: item_a },
      CsmParam { id: 1, payload: item_b },
    ]);
    let items = decode_queue_snapshot(blob).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].title.as_deref(), Some("A"));
    assert_eq!(items[0].artist.as_deref(), Some("AArt"));
    assert_eq!(items[0].artwork_id, Some(1));
    assert_eq!(items[0].persistent_id, Some(0x0011_2233_4455_6677u64));
    assert_eq!(items[1].title.as_deref(), Some("B"));
    assert_eq!(items[1].duration_ms, Some(123_000));
  }

  #[test]
  fn queue_snapshot_skips_malformed_entry_and_continues() {
    let good = build_group(&[(MEDIA_ITEM_TITLE, b"OK\0")]);
    let bad_payload = Bytes::from_static(&[0xFF, 0xFE]); // not a valid param block
    let blob = encode_param_block(vec![
      CsmParam { id: 0, payload: good },
      CsmParam {
        id: 1,
        payload: bad_payload,
      },
    ]);
    let items = decode_queue_snapshot(blob).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title.as_deref(), Some("OK"));
  }

  #[test]
  fn queue_snapshot_empty_blob_yields_empty_vec() {
    let items = decode_queue_snapshot(Bytes::new()).unwrap();
    assert!(items.is_empty());
  }
}
