//! Typed CSMs for the iAP2 NowPlaying surface.
//!
//! Three messages live here:
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
pub const SENT_BY_ACCESSORY: &[u16] = &[StartNowPlayingUpdates::CSM_MSG_ID, StopNowPlayingUpdates::CSM_MSG_ID];

/// CSMs the accessory accepts in this layer. Identification merges
/// these into param 7 of `IdentificationInformation`.
pub const RECEIVED_BY_ACCESSORY: &[u16] = &[NowPlayingUpdate::CSM_MSG_ID];

/// MediaItem attribute sub-ids the accessory subscribes to. iPhone
/// only sends these back. Drops the `0x04` sub-id whose purpose has
/// not been pinned down in cleanroom analysis - subscribing to fields
/// we don't render costs us inbound traffic for no benefit.
pub const MEDIA_ITEM_SUBSCRIBE: &[u16] = &[
  MEDIA_ITEM_PERSISTENT_ID,
  MEDIA_ITEM_TITLE,
  MEDIA_ITEM_ALBUM,
  MEDIA_ITEM_ARTIST,
  MEDIA_ITEM_LIKED,
  MEDIA_ITEM_ARTWORK_ID,
];

/// Playback attribute sub-ids the accessory subscribes to.
pub const PLAYBACK_SUBSCRIBE: &[u16] = &[
  PLAYBACK_STATE,
  PLAYBACK_POSITION_MS,
  PLAYBACK_SHUFFLE,
  PLAYBACK_REPEAT,
  PLAYBACK_APP_DISPLAY_NAME,
  PLAYBACK_LIBRARY_UNIQUE_ID,
  PLAYBACK_APP_BUNDLE,
];

const MEDIA_ITEM_PERSISTENT_ID: u16 = 0x00;
const MEDIA_ITEM_TITLE: u16 = 0x01;
const MEDIA_ITEM_ALBUM: u16 = 0x06;
const MEDIA_ITEM_ARTIST: u16 = 0x0C;
const MEDIA_ITEM_ARTIST_ALT: u16 = 0x0E;
const MEDIA_ITEM_LIKED: u16 = 0x17;
const MEDIA_ITEM_ARTWORK_ID: u16 = 0x1A;

const PLAYBACK_STATE: u16 = 0x00;
const PLAYBACK_POSITION_MS: u16 = 0x01;
const PLAYBACK_SHUFFLE: u16 = 0x05;
const PLAYBACK_REPEAT: u16 = 0x06;
const PLAYBACK_APP_DISPLAY_NAME: u16 = 0x07;
const PLAYBACK_LIBRARY_UNIQUE_ID: u16 = 0x08;
const PLAYBACK_APP_BUNDLE: u16 = 0x10;

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

/// Per-track attributes carried inside the `NowPlayingUpdate` param-0
/// group. All fields optional; iPhone sends only the ones that
/// changed since the last update.
///
/// `artist` collapses sub-ids `0x0C` and `0x0E` into one field. Stock
/// subscribes only to `0x0C` so `0x0E` rarely shows up, but the
/// decode tolerates either (preferring `0x0C` when both are present).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MediaItemAttributes {
  pub persistent_id: Option<u64>,
  pub title: Option<String>,
  pub album: Option<String>,
  pub artist: Option<String>,
  pub liked: Option<bool>,
  pub artwork_id: Option<u8>,
}

impl MediaItemAttributes {
  fn decode_group(payload: Bytes) -> Result<Self, CsmDecodeError> {
    let mut params = decode_param_block(payload)?;
    let persistent_id = take_optional_be_u64(MEDIA_ITEM_PERSISTENT_ID, &mut params)?;
    let title = Option::<String>::decode_field(MEDIA_ITEM_TITLE, &mut params)?;
    let album = Option::<String>::decode_field(MEDIA_ITEM_ALBUM, &mut params)?;
    let artist_primary = Option::<String>::decode_field(MEDIA_ITEM_ARTIST, &mut params)?;
    let artist_alt = Option::<String>::decode_field(MEDIA_ITEM_ARTIST_ALT, &mut params)?;
    let liked = take_optional_presence_bool(MEDIA_ITEM_LIKED, &mut params)?;
    let artwork_id = take_optional_be_u8(MEDIA_ITEM_ARTWORK_ID, &mut params)?;
    Ok(Self {
      persistent_id,
      title,
      album,
      artist: artist_primary.or(artist_alt),
      liked,
      artwork_id,
    })
  }
}

/// Per-playback-session attributes carried inside the
/// `NowPlayingUpdate` param-1 group. `app_bundle` is the iOS app's
/// bundle identifier (e.g. `"com.spotify.client"`) - the most
/// reliable signal for "what audio app is foregrounded right now."
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlaybackAttributes {
  pub state: Option<PlaybackState>,
  pub position_ms: Option<u32>,
  pub shuffle: Option<bool>,
  pub repeat: Option<RepeatMode>,
  pub app_display_name: Option<String>,
  pub library_unique_id: Option<String>,
  pub app_bundle: Option<String>,
}

impl PlaybackAttributes {
  fn decode_group(payload: Bytes) -> Result<Self, CsmDecodeError> {
    let mut params = decode_param_block(payload)?;
    let state = take_optional_be_u8(PLAYBACK_STATE, &mut params)?.map(PlaybackState::from_byte);
    let position_ms = take_optional_be_u32(PLAYBACK_POSITION_MS, &mut params)?;
    let shuffle = take_optional_presence_bool(PLAYBACK_SHUFFLE, &mut params)?;
    let repeat = take_optional_be_u8(PLAYBACK_REPEAT, &mut params)?.map(RepeatMode::from_byte);
    let app_display_name = Option::<String>::decode_field(PLAYBACK_APP_DISPLAY_NAME, &mut params)?;
    let library_unique_id = Option::<String>::decode_field(PLAYBACK_LIBRARY_UNIQUE_ID, &mut params)?;
    let app_bundle = Option::<String>::decode_field(PLAYBACK_APP_BUNDLE, &mut params)?;
    Ok(Self {
      state,
      position_ms,
      shuffle,
      repeat,
      app_display_name,
      library_unique_id,
      app_bundle,
    })
  }
}

/// On the wire the playback-state byte is documented as `0` paused
/// and `1` playing, with any other value treated as paused (per the
/// stock app's `& 0xFD` mask). Anything that isn't a clean `1` falls
/// into `Paused` here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
  Paused,
  Playing,
}

impl PlaybackState {
  fn from_byte(byte: u8) -> Self {
    match byte {
      1 => Self::Playing,
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
    Self::decode_group(payload)
  }
  fn default_group() -> Self {
    Self::default()
  }
}

impl GroupDecode for PlaybackAttributes {
  fn decode_group(payload: Bytes) -> Result<Self, CsmDecodeError> {
    Self::decode_group(payload)
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
  fn now_playing_decodes_full_media_and_playback() {
    let media_payload = build_group(&[
      (MEDIA_ITEM_PERSISTENT_ID, &0x0011_2233_4455_6677u64.to_be_bytes()),
      (MEDIA_ITEM_TITLE, b"Hello\0"),
      (MEDIA_ITEM_ALBUM, b"World\0"),
      (MEDIA_ITEM_ARTIST, b"Artist\0"),
      (MEDIA_ITEM_LIKED, &[]),
      (MEDIA_ITEM_ARTWORK_ID, &[7]),
    ]);
    let playback_payload = build_group(&[
      (PLAYBACK_STATE, &[1]),
      (PLAYBACK_POSITION_MS, &12_345u32.to_be_bytes()),
      (PLAYBACK_SHUFFLE, &[]),
      (PLAYBACK_REPEAT, &[2]),
      (PLAYBACK_APP_DISPLAY_NAME, b"Spotify\0"),
      (PLAYBACK_APP_BUNDLE, b"com.spotify.client\0"),
    ]);
    let frame = frame_with_groups(Some(media_payload), Some(playback_payload));
    let update: NowPlayingUpdate = frame.try_into().unwrap();
    let media = update.media_item.expect("media_item");
    assert_eq!(media.persistent_id, Some(0x0011_2233_4455_6677u64));
    assert_eq!(media.title.as_deref(), Some("Hello"));
    assert_eq!(media.album.as_deref(), Some("World"));
    assert_eq!(media.artist.as_deref(), Some("Artist"));
    assert_eq!(media.liked, Some(true));
    assert_eq!(media.artwork_id, Some(7));
    let play = update.playback.expect("playback");
    assert_eq!(play.state, Some(PlaybackState::Playing));
    assert_eq!(play.position_ms, Some(12_345));
    assert_eq!(play.shuffle, Some(true));
    assert_eq!(play.repeat, Some(RepeatMode::All));
    assert_eq!(play.app_display_name.as_deref(), Some("Spotify"));
    assert_eq!(play.app_bundle.as_deref(), Some("com.spotify.client"));
  }

  #[test]
  fn artist_falls_back_to_alternate_when_only_alt_present() {
    let media_payload = build_group(&[(MEDIA_ITEM_ARTIST_ALT, b"AltArtist\0")]);
    let frame = frame_with_groups(Some(media_payload), None);
    let update: NowPlayingUpdate = frame.try_into().unwrap();
    let media = update.media_item.expect("media_item");
    assert_eq!(media.artist.as_deref(), Some("AltArtist"));
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
    assert_eq!(play.state, Some(PlaybackState::Paused));
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
}
