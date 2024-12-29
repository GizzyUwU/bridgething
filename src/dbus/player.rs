use super::{media_player1::MediaPlayer1Track, DBusError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerShuffle {
  On,
  Off,
}

impl TryFrom<String> for PlayerShuffle {
  type Error = DBusError;

  fn try_from(value: String) -> Result<Self, Self::Error> {
    match value.as_str() {
      "on" => Ok(PlayerShuffle::On),
      "off" => Ok(PlayerShuffle::Off),
      _ => Err(DBusError::Deserialization(value)),
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerRepeat {
  On,
  Off,
}

impl TryFrom<String> for PlayerRepeat {
  type Error = DBusError;

  fn try_from(value: String) -> Result<Self, Self::Error> {
    match value.as_str() {
      "on" => Ok(PlayerRepeat::On),
      "off" => Ok(PlayerRepeat::Off),
      _ => Err(DBusError::Deserialization(value)),
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerStatus {
  Playing,
  Paused,
}

impl TryFrom<String> for PlayerStatus {
  type Error = DBusError;

  fn try_from(value: String) -> Result<Self, Self::Error> {
    match value.as_str() {
      "playing" => Ok(PlayerStatus::Playing),
      "paused" => Ok(PlayerStatus::Paused),
      _ => Err(DBusError::Deserialization(value)),
    }
  }
}

#[derive(Debug, Clone)]
pub struct PlayerTrack {
  pub title: String,
  pub artist: String,
  pub album: String,
  pub duration: u32,
  pub track_number: u32,
  pub number_of_tracks: u32,
}

impl TryFrom<MediaPlayer1Track> for PlayerTrack {
  type Error = DBusError;

  fn try_from(value: MediaPlayer1Track) -> Result<Self, Self::Error> {
    Ok(Self {
      title: value
        .get("Title")
        .ok_or(DBusError::Deserialization("Title".to_string()))?
        .downcast_ref()?,
      artist: value
        .get("Artist")
        .ok_or(DBusError::Deserialization("Artist".to_string()))?
        .downcast_ref()?,
      album: value
        .get("Album")
        .ok_or(DBusError::Deserialization("Album".to_string()))?
        .downcast_ref()?,
      duration: value
        .get("Duration")
        .ok_or(DBusError::Deserialization("Duration".to_string()))?
        .downcast_ref()?,
      track_number: value
        .get("TrackNumber")
        .ok_or(DBusError::Deserialization("TrackNumber".to_string()))?
        .downcast_ref()?,
      number_of_tracks: value
        .get("NumberOfTracks")
        .ok_or(DBusError::Deserialization("NumberOfTracks".to_string()))?
        .downcast_ref()?,
    })
  }
}
