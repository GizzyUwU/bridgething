use super::{media_player1::MediaPlayer1Track, DBusError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerShuffle {
  On,
  Off,
}

impl Default for PlayerShuffle {
  fn default() -> Self {
    Self::Off
  }
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

impl From<&PlayerShuffle> for &str {
  fn from(val: &PlayerShuffle) -> Self {
    match *val {
      PlayerShuffle::On => "on",
      PlayerShuffle::Off => "off",
    }
  }
}

impl From<bool> for PlayerShuffle {
  fn from(shuffle: bool) -> Self {
    match shuffle {
      true => Self::On,
      false => Self::Off,
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerRepeat {
  On,
  Off,
}

impl Default for PlayerRepeat {
  fn default() -> Self {
    Self::Off
  }
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

impl From<&PlayerRepeat> for &str {
  fn from(val: &PlayerRepeat) -> Self {
    match *val {
      PlayerRepeat::On => "on",
      PlayerRepeat::Off => "off",
    }
  }
}

impl From<bool> for PlayerRepeat {
  fn from(repeat: bool) -> Self {
    match repeat {
      true => Self::On,
      false => Self::Off,
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerStatus {
  Playing,
  Paused,
}

impl Default for PlayerStatus {
  fn default() -> Self {
    Self::Paused
  }
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
  pub artists: Vec<String>,
  pub album: String,
  pub duration: usize,
  pub track_number: usize,
  pub number_of_tracks: usize,
}

impl Default for PlayerTrack {
  fn default() -> Self {
    Self {
      title: "BridgeThing".to_string(),
      artists: vec!["Thing Labs".to_string()],
      album: "Joey Eamigh".to_string(),
      duration: 5000,
      track_number: 1,
      number_of_tracks: 1,
    }
  }
}

impl TryFrom<MediaPlayer1Track> for PlayerTrack {
  type Error = DBusError;

  fn try_from(value: MediaPlayer1Track) -> Result<Self, Self::Error> {
    Ok(Self {
      title: value
        .get("Title")
        .ok_or(DBusError::Deserialization("Title".to_string()))?
        .downcast_ref()?,
      artists: value
        .get("Artist")
        .ok_or(DBusError::Deserialization("Artist".to_string()))?
        .downcast_ref::<String>()?
        .split(',')
        .map(|s| s.trim().to_string())
        .collect(),
      album: value
        .get("Album")
        .ok_or(DBusError::Deserialization("Album".to_string()))?
        .downcast_ref()?,
      duration: value
        .get("Duration")
        .ok_or(DBusError::Deserialization("Duration".to_string()))?
        .downcast_ref::<u32>()?
        .try_into()?,
      track_number: value
        .get("TrackNumber")
        .ok_or(DBusError::Deserialization("TrackNumber".to_string()))?
        .downcast_ref::<u32>()?
        .try_into()?,
      number_of_tracks: value
        .get("NumberOfTracks")
        .ok_or(DBusError::Deserialization("NumberOfTracks".to_string()))?
        .downcast_ref::<u32>()?
        .try_into()?,
    })
  }
}
