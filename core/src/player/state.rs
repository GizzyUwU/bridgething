use libbridgething::{
  Device, PlaybackOptions, PlaybackQueue, PlaybackRestrictions, ServerEventType, Track, server::ServerPlayerEvent,
};

use crate::server::ClientMan;

use super::{PlayerResult, dbus::DBusPlayerEvent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepeatState {
  Off,
  All,
  Track,
}

impl Default for RepeatState {
  fn default() -> Self {
    Self::Off
  }
}

#[derive(Debug, Clone)]
pub struct PlayerState {
  client_man: ClientMan,

  pub device: Option<Device>,
  pub playing: bool,

  pub context_title: String,
  pub context_id: Option<String>,

  pub position_ms: usize,
  pub playback_speed: f64,

  pub track: Option<Track>,
  pub queue: Option<PlaybackQueue>,

  pub volume: u8,
  pub options: PlaybackOptions,
  pub restrictions: PlaybackRestrictions,
}

impl PlayerState {
  pub fn new(client_man: ClientMan) -> Self {
    Self {
      client_man,

      device: None,
      playing: false,

      context_title: "BridgeThing".to_string(),
      context_id: None,

      position_ms: 0,
      playback_speed: 1.0,

      track: None,
      queue: None,

      volume: 100,
      options: PlaybackOptions::default(),
      restrictions: PlaybackRestrictions::default(),
    }
  }

  // TODO: change how this works so that it doesn't spam the client with new events
  pub(crate) async fn handle_dbus_event(&mut self, event: DBusPlayerEvent) -> PlayerResult<()> {
    tracing::trace!("new dbus message: {:?}", &event);

    match event {
      DBusPlayerEvent::Status(status) => {
        self.playing = status.into();
      }
      DBusPlayerEvent::Track(track) => {
        // if self.state.track.title != track.title {
        //   if let Some(art) = &self.art {
        //     art.fetch(&track.image_id(), None).await;
        //   }
        // }
        self.track = Some(track.into());
      }
      DBusPlayerEvent::Position(position) => {
        self.position_ms = position;
      }
      DBusPlayerEvent::Shuffle(shuffle) => {
        self.options.shuffle = shuffle.into();
      }
      DBusPlayerEvent::Repeat(repeat) => {
        self.options.repeat = repeat.into();
      }
    }

    self.send_state().await?;

    Ok(())
  }

  pub async fn send_state(&self) -> PlayerResult<()> {
    self
      .client_man
      .broadcast(self.to_send_state(), ServerEventType::Info)
      .await?;
    self
      .client_man
      .broadcast(self.to_send_queue(), ServerEventType::Info)
      .await?;

    Ok(())
  }

  pub fn to_send_state(&self) -> ServerPlayerEvent {
    ServerPlayerEvent::PlayerState {
      context_id: self
        .context_id
        .clone()
        .unwrap_or("bridgething:context:fake".to_string()),
      context_title: self.context_title.clone(),
      is_paused: !self.playing,
      playback_options: self.options.clone(),
      playback_position: self.position_ms,
      playback_restrictions: self.restrictions.clone(),
      playback_speed: self.playback_speed,
      track: self.track.clone().unwrap_or_default(),
    }
  }

  pub fn to_send_queue(&self) -> ServerPlayerEvent {
    ServerPlayerEvent::Queue {
      current: self.track.clone().unwrap_or_default(),
      previous: vec![],
      next: vec![],
    }
  }
}
