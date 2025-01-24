use std::collections::HashMap;

use libbridgething::{client::ClientInteractionCommand, server::ServerPlayerEvent, stock::StockSetPreset};

use crate::{
  msg::stock::{ChildItem, ChildMeta, StockInterAppSend, StockInterAppSendPayload, StockPermissionsSend},
  state::State,
};

use super::{Handler, HandlerResult, MsgHandle};

#[derive(Debug)]
pub struct InteractionHandler<'a> {
  handle: MsgHandle,
  state: &'a mut State,
  stock_msg_id: Option<usize>,
}

impl<'a> InteractionHandler<'a> {
  pub fn new(handler: Handler<'a>, stock_msg_id: Option<usize>) -> Self {
    let mut handle = handler.handle;
    handle.stock_msg_id = stock_msg_id;

    Self {
      handle,
      state: handler.state,
      stock_msg_id,
    }
  }

  pub async fn handle(self, msg: ClientInteractionCommand) -> HandlerResult {
    tracing::debug!(
      "({}) handling interaction message: id: {:?}; stock_msg_id: {:?}",
      &self.handle.from,
      &self.handle.id,
      &self.handle.stock_msg_id
    );

    match msg {
      ClientInteractionCommand::GetImage { id } => self.get_image(id).await,
      ClientInteractionCommand::GetThumbnailImage { id } => self.get_thumbnail_image(id).await,

      ClientInteractionCommand::GetNextTracks => self.get_next_tracks().await,
      ClientInteractionCommand::PhoneAnswer => self.phone_answer().await,
      ClientInteractionCommand::PhoneDecline => self.phone_decline().await,
      ClientInteractionCommand::PhoneCallImage { phone_number } => self.phone_call_image(phone_number).await,
      ClientInteractionCommand::PhoneCallMessage { phone_number, message } => {
        self.phone_call_message(phone_number, message).await
      }
      ClientInteractionCommand::IncreaseVolume => self.increase_volume().await,
      ClientInteractionCommand::DecreaseVolume => self.decrease_volume().await,
      ClientInteractionCommand::SkipToIndex { index } => self.skip_to_index(index).await,
      ClientInteractionCommand::SkipNext => self.skip_next().await,
      ClientInteractionCommand::SkipPrev { allow_seeking } => self.skip_prev(allow_seeking).await,
      ClientInteractionCommand::SeekTo { position } => self.seek_to(position).await,
      ClientInteractionCommand::Pause => self.pause().await,
      ClientInteractionCommand::Resume => self.resume().await,
      ClientInteractionCommand::SetShuffle { shuffle } => self.set_shuffle(shuffle).await,
      ClientInteractionCommand::SetRepeat { repeat_mode } => self.set_repeat(repeat_mode).await,
      ClientInteractionCommand::SpotifyGetChildren {
        parent_id,
        limit,
        offset,
      } => self.spotify_get_children(parent_id, limit, offset).await,
      ClientInteractionCommand::__LegacySpotifyGetHome { limit, limit_overrides } => {
        self.spotify_get_home(limit, limit_overrides).await
      }
      ClientInteractionCommand::__LegacySpotifyGetPermissions => self.spotify_get_permissions().await,
      ClientInteractionCommand::SpotifyGetPodcast { uri, limit, offset } => {
        self.spotify_get_podcast(uri, limit, offset).await
      }
      ClientInteractionCommand::__LegacySpotifyGetPresets => self.spotify_get_presets().await,
      ClientInteractionCommand::SpotifyGetSaved { id } => self.spotify_get_saved(id).await,
      ClientInteractionCommand::__LegacySpotifyGetTips => self.spotify_get_tips().await,
      ClientInteractionCommand::__LegacySpotifyGetTts { file } => self.spotify_get_tts(file).await,
      ClientInteractionCommand::SpotifyPlayPodcastTrailer { uri } => self.spotify_play_podcast_trailer(uri).await,
      ClientInteractionCommand::SpotifyQueueUri { uri } => self.spotify_queue_uri(uri).await,
      ClientInteractionCommand::SpotifySetPodcastPlaybackSpeed { playback_speed } => {
        self.spotify_set_podcast_playback_speed(playback_speed).await
      }
      ClientInteractionCommand::__LegacySpotifySetPreset { presets } => self.spotify_set_preset(presets).await,
      ClientInteractionCommand::SpotifySetSaved { id, uri, saved } => self.spotify_set_saved(id, uri, saved).await,
      ClientInteractionCommand::__LegacySpotifySummonDj => self.spotify_summon_dj().await,
      ClientInteractionCommand::SpotifyPlayUri {
        uri,
        feature_identifier,
        interaction_id,
        skip_to_uri,
        skip_to_uid,
      } => {
        self
          .spotify_play_uri(uri, feature_identifier, interaction_id, skip_to_uri, skip_to_uid)
          .await
      }
    }
  }

  async fn get_image(self, id: String) -> HandlerResult {
    tracing::debug!("({}) getting image with id: {}", &self.handle.from, id);
    let Some(player) = &self.state.player else {
      return Ok(());
    };

    player.request_cover_art(self.handle).await;
    Ok(())
  }

  async fn get_thumbnail_image(self, id: String) -> HandlerResult {
    tracing::debug!("({}) getting thumbnail image for id: {}", &self.handle.from, id);
    let Some(player) = &self.state.player else {
      return Ok(());
    };

    player.request_cover_art(self.handle).await;
    Ok(())
  }

  async fn get_next_tracks(&self) -> HandlerResult {
    tracing::debug!("({}) getting next tracks", &self.handle.from);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn phone_answer(&self) -> HandlerResult {
    tracing::debug!("({}) answering phone", &self.handle.from);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn phone_decline(&self) -> HandlerResult {
    tracing::debug!("({}) declining phone", &self.handle.from);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn phone_call_image(&self, phone_number: String) -> HandlerResult {
    tracing::debug!(
      "({}) getting phone call image for number: {}",
      &self.handle.id,
      phone_number
    );
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn phone_call_message(&self, phone_number: String, message: String) -> HandlerResult {
    tracing::debug!(
      "({}) sending phone call message to number: {}, message: {}",
      &self.handle.id,
      phone_number,
      message
    );
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn increase_volume(&self) -> HandlerResult {
    tracing::debug!("({}) increasing volume", &self.handle.from);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn decrease_volume(&self) -> HandlerResult {
    tracing::debug!("({}) decreasing volume", &self.handle.from);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn skip_to_index(&self, index: usize) -> HandlerResult {
    tracing::debug!("({}) skipping to index: {}", &self.handle.from, index);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn skip_next(&self) -> HandlerResult {
    tracing::debug!("({}) skipping to next track", &self.handle.from);
    let Some(player) = &self.state.player else {
      return Ok(());
    };

    Ok(player.next().await?)
  }

  async fn skip_prev(&self, allow_seeking: bool) -> HandlerResult {
    tracing::debug!(
      "({}) skipping to previous track, allow seeking: {}",
      &self.handle.id,
      allow_seeking
    );
    let Some(player) = &self.state.player else {
      return Ok(());
    };

    Ok(player.prev().await?)
  }

  async fn seek_to(&self, position: usize) -> HandlerResult {
    tracing::debug!("({}) seeking to position: {}", &self.handle.from, position);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn pause(&self) -> HandlerResult {
    tracing::debug!("({}) pausing playback", &self.handle.from);
    let Some(player) = &self.state.player else {
      return Ok(());
    };

    Ok(player.pause().await?)
  }

  async fn resume(&self) -> HandlerResult {
    tracing::debug!("({}) resuming playback", &self.handle.from);
    let Some(player) = &self.state.player else {
      return Ok(());
    };

    Ok(player.play().await?)
  }

  async fn set_shuffle(&self, shuffle: bool) -> HandlerResult {
    tracing::debug!("({}) setting shuffle to: {}", &self.handle.from, shuffle);
    let Some(player) = &self.state.player else {
      return Ok(());
    };

    Ok(player.shuffle(shuffle.into()).await?)
  }

  async fn set_repeat(&self, repeat: bool) -> HandlerResult {
    tracing::debug!("({}) setting repeat to: {}", &self.handle.from, repeat);
    let Some(player) = &self.state.player else {
      return Ok(());
    };

    Ok(player.repeat(repeat.into()).await?)
  }

  async fn spotify_get_children(&self, parent_id: String, limit: usize, offset: Option<usize>) -> HandlerResult {
    tracing::debug!(
      "({}) getting Spotify children for parent id: {}, limit: {}, offset: {:?}",
      &self.handle.id,
      parent_id,
      limit,
      offset
    );

    // TODO: remove testing code
    self
      .handle
      .send_stock(StockInterAppSend::new(
        self.stock_msg_id,
        StockInterAppSendPayload::ItemChildren {
          limit: 10000,
          offset: 0,
          total: 1,
          items: vec![ChildItem {
            id: "spotify:track:bridgething".to_string(),
            uri: "spotify:track:bridgething".to_string(),
            image_id: "spotify:image:bridgething".to_string(),
            title: "BridgeThing".to_string(),
            subtitle: "Thing Labs".to_string(),
            playable: true,
            has_children: false,
            available_offline: false,
            metadata: ChildMeta {
              is_explicit_content: false,
              is_19_plus_content: false,
              duration_ms: 5_000_000,
            },
          }],
        },
      ))
      .await?;

    Ok(())
  }

  async fn spotify_get_home(&self, limit: usize, limit_overrides: HashMap<String, usize>) -> HandlerResult {
    tracing::debug!(
      "({}) getting Spotify home with limit: {}, limit overrides: {:?}",
      &self.handle.id,
      limit,
      limit_overrides
    );
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn spotify_get_permissions(&self) -> HandlerResult {
    tracing::debug!("({}) handling Spotify permissions request", &self.handle.from);

    self
      .handle
      .send_stock(StockPermissionsSend::DevicePermissions {
        can_use_superbird: true,
        can_play_on_demand: None,
      })
      .await?;
    self
      .handle
      .send_stock(StockInterAppSend::new(
        self.stock_msg_id,
        StockInterAppSendPayload::Permissions {
          can_use_superbird: true,
        },
      ))
      .await?;

    Ok(())
  }

  async fn spotify_get_podcast(&self, uri: String, limit: Option<usize>, offset: Option<usize>) -> HandlerResult {
    tracing::debug!(
      "({}) getting Spotify podcast for uri: {}, limit: {:?}, offset: {:?}",
      &self.handle.id,
      uri,
      limit,
      offset
    );
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn spotify_get_presets(&self) -> HandlerResult {
    tracing::debug!("({}) getting Spotify presets", &self.handle.from);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn spotify_get_saved(&self, id: String) -> HandlerResult {
    tracing::debug!("({}) getting Spotify saved item for id: {}", &self.handle.from, id);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn spotify_get_tips(&self) -> HandlerResult {
    tracing::debug!("({}) getting Spotify tips", &self.handle.from);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn spotify_get_tts(&self, file: String) -> HandlerResult {
    tracing::debug!("({}) getting Spotify TTS for file: {}", &self.handle.from, file);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn spotify_play_podcast_trailer(&self, uri: String) -> HandlerResult {
    tracing::debug!(
      "({}) playing Spotify podcast trailer for uri: {}",
      &self.handle.from,
      uri
    );
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn spotify_queue_uri(&self, uri: String) -> HandlerResult {
    tracing::debug!("({}) queuing Spotify uri: {}", &self.handle.from, uri);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn spotify_set_podcast_playback_speed(&self, playback_speed: usize) -> HandlerResult {
    tracing::debug!(
      "({}) setting Spotify podcast playback speed to: {}",
      &self.handle.id,
      playback_speed
    );
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn spotify_set_preset(&self, presets: Vec<StockSetPreset>) -> HandlerResult {
    tracing::debug!("({}) setting Spotify presets: {:?}", &self.handle.from, presets);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn spotify_set_saved(&self, id: Option<String>, uri: Option<String>, saved: bool) -> HandlerResult {
    tracing::debug!(
      "({}) setting Spotify saved item for id: {:?}, uri: {:?}, saved: {}",
      &self.handle.id,
      id,
      uri,
      saved
    );
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn spotify_summon_dj(&self) -> HandlerResult {
    tracing::debug!("({}) summoning Spotify DJ", &self.handle.from);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn spotify_play_uri(
    &self,
    uri: String,
    feature_identifier: String,
    interaction_id: Option<String>,
    skip_to_uri: Option<String>,
    skip_to_uid: Option<String>,
  ) -> HandlerResult {
    tracing::debug!("({}) playing Spotify uri: {}, feature identifier: {}, interaction id: {:?}, skip to uri: {:?}, skip to uid: {:?}", &self.handle.from, uri, feature_identifier, interaction_id, skip_to_uri, skip_to_uid);
    // Ok(self.handle.respond().await?)
    Ok(())
  }
}
