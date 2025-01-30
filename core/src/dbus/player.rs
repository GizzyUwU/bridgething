use libbridgething::Track;

use super::{media_player1::MediaPlayer1Track, DBusError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DBusPlayerShuffle {
  On,
  Off,
}

impl Default for DBusPlayerShuffle {
  fn default() -> Self {
    Self::Off
  }
}

impl TryFrom<String> for DBusPlayerShuffle {
  type Error = DBusError;

  fn try_from(value: String) -> Result<Self, Self::Error> {
    match value.as_str() {
      "on" => Ok(DBusPlayerShuffle::On),
      "off" => Ok(DBusPlayerShuffle::Off),
      _ => Err(DBusError::Deserialization(value)),
    }
  }
}

impl From<&DBusPlayerShuffle> for &str {
  fn from(val: &DBusPlayerShuffle) -> Self {
    match *val {
      DBusPlayerShuffle::On => "on",
      DBusPlayerShuffle::Off => "off",
    }
  }
}

impl From<bool> for DBusPlayerShuffle {
  fn from(shuffle: bool) -> Self {
    match shuffle {
      true => Self::On,
      false => Self::Off,
    }
  }
}

impl From<DBusPlayerShuffle> for bool {
  fn from(val: DBusPlayerShuffle) -> Self {
    val == DBusPlayerShuffle::On
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DBusPlayerRepeat {
  On,
  Off,
}

impl From<DBusPlayerRepeat> for usize {
  fn from(val: DBusPlayerRepeat) -> Self {
    if val == DBusPlayerRepeat::On {
      1
    } else {
      0
    }
  }
}

impl Default for DBusPlayerRepeat {
  fn default() -> Self {
    Self::Off
  }
}

impl TryFrom<String> for DBusPlayerRepeat {
  type Error = DBusError;

  fn try_from(value: String) -> Result<Self, Self::Error> {
    match value.as_str() {
      "on" => Ok(DBusPlayerRepeat::On),
      "off" => Ok(DBusPlayerRepeat::Off),
      _ => Err(DBusError::Deserialization(value)),
    }
  }
}

impl From<&DBusPlayerRepeat> for &str {
  fn from(val: &DBusPlayerRepeat) -> Self {
    match *val {
      DBusPlayerRepeat::On => "on",
      DBusPlayerRepeat::Off => "off",
    }
  }
}

impl From<bool> for DBusPlayerRepeat {
  fn from(repeat: bool) -> Self {
    match repeat {
      true => Self::On,
      false => Self::Off,
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DBusPlayerStatus {
  Playing,
  Paused,
}

impl Default for DBusPlayerStatus {
  fn default() -> Self {
    Self::Paused
  }
}

impl TryFrom<String> for DBusPlayerStatus {
  type Error = DBusError;

  fn try_from(value: String) -> Result<Self, Self::Error> {
    match value.as_str() {
      "playing" => Ok(DBusPlayerStatus::Playing),
      "paused" => Ok(DBusPlayerStatus::Paused),
      _ => Err(DBusError::Deserialization(value)),
    }
  }
}

#[derive(Debug, Clone)]
pub struct DBusPlayerTrack {
  pub title: String,
  pub artists: Vec<String>,
  pub album: String,
  pub duration: usize,
  pub track_number: usize,
  pub number_of_tracks: usize,
}

impl DBusPlayerTrack {
  pub fn track_id(&self) -> String {
    format!(
      "bridgething:track:{}:{}:{}",
      self
        .artists
        .first()
        .unwrap_or(&String::from("unknown_artist"))
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == ' ')
        .collect::<String>()
        .replace(' ', "_"),
      self
        .album
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == ' ')
        .collect::<String>()
        .replace(' ', "_"),
      self
        .title
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == ' ')
        .collect::<String>()
        .replace(' ', "_")
    )
  }

  pub fn image_id(&self) -> String {
    format!("{}:image", self.track_id())
  }
}

impl Default for DBusPlayerTrack {
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

impl From<DBusPlayerTrack> for Track {
  fn from(val: DBusPlayerTrack) -> Self {
    Track {
      id: val.track_id(),
      image_id: val.image_id(),
      name: val.title,
      artist: val.artists.first().cloned().unwrap_or_default().into(),
      artists: val.artists.into_iter().map(Into::into).collect(),
      duration_ms: val.duration,
      saved: false,
      album: val.album.into(),
    }
  }
}

impl TryFrom<MediaPlayer1Track> for DBusPlayerTrack {
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
