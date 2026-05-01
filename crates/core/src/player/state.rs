use libbridgething::{
  Device, NowPlayingUpdate, PlaybackOptions, PlaybackQueue, PlaybackRestrictions, ServerEventType, Track,
  server::ServerPlayerEvent,
};

use super::{PlayerResult, dbus::DBusPlayerEvent};
use crate::http::ClientMan;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RepeatState {
  #[default]
  Off,
  All,
  Track,
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

  // resets the track on persistent_id change so fields the producer
  // didn't re-send across the boundary don't leak
  pub(crate) async fn apply_now_playing(&mut self, update: NowPlayingUpdate) -> PlayerResult<()> {
    let NowPlayingUpdate { media_item, playback } = update;

    if let Some(media) = media_item {
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
    }

    if let Some(play) = playback {
      if let Some(playing) = play.playing {
        self.playing = playing;
      }
      if let Some(position) = play.position_ms {
        self.position_ms = position as usize;
      }
      if let Some(shuffle) = play.shuffle {
        self.options.shuffle = shuffle;
      }
      if let Some(repeat) = play.repeat {
        self.options.repeat = repeat;
      }
      if let Some(name) = play.app_display_name {
        self.context_title = name;
      }
      if let Some(bundle) = play.app_bundle {
        self.context_id = Some(bundle);
      }
    }

    self.send_state().await?;
    Ok(())
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
      .broadcast(self.to_send_state(), ServerEventType::Event)
      .await?;
    self
      .client_man
      .broadcast(self.to_send_queue(), ServerEventType::Event)
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
