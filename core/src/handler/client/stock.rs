use std::collections::HashMap;

use libbridgething::{client::ClientLegacyStockCommand, stock::StockSetPreset};

use crate::msg::stock::{ChildItem, ChildMeta, StockInterAppSend, StockInterAppSendPayload, StockPermissionsSend};

use super::{HandlerResult, MsgHandle};

#[derive(Debug)]
pub struct LegacyStockHandler {
  handle: MsgHandle,
}

impl LegacyStockHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: ClientLegacyStockCommand) -> HandlerResult {
    tracing::debug!(
      "({}) handling legacy stock message: id: {:?}; stock_msg_id: {:?}",
      &self.handle.from,
      &self.handle.id,
      &self.handle.stock_msg_id
    );

    match msg {
      ClientLegacyStockCommand::GetImage { id } => self.get_image(id).await,
      ClientLegacyStockCommand::GetThumbnailImage { id } => self.get_thumbnail_image(id).await,
      ClientLegacyStockCommand::GetNextTracks => self.get_next_tracks().await,

      // spotify things
      ClientLegacyStockCommand::SpotifyGetChildren {
        parent_id,
        limit,
        offset,
      } => self.spotify_get_children(parent_id, limit, offset).await,
      ClientLegacyStockCommand::SpotifyGetHome { limit, limit_overrides } => {
        self.spotify_get_home(limit, limit_overrides).await
      }
      ClientLegacyStockCommand::SpotifyGetPermissions => self.spotify_get_permissions().await,
      ClientLegacyStockCommand::SpotifyGetPodcast { uri, limit, offset } => {
        self.spotify_get_podcast(uri, limit, offset).await
      }
      ClientLegacyStockCommand::SpotifyGetPresets => self.spotify_get_presets().await,
      ClientLegacyStockCommand::SpotifyGetSaved { id } => self.spotify_get_saved(id).await,
      ClientLegacyStockCommand::SpotifyGetTips => self.spotify_get_tips().await,
      ClientLegacyStockCommand::SpotifyGetTts { file } => self.spotify_get_tts(file).await,
      ClientLegacyStockCommand::SpotifyPlayPodcastTrailer { uri } => self.spotify_play_podcast_trailer(uri).await,
      ClientLegacyStockCommand::SpotifyQueueUri { uri } => self.spotify_queue_uri(uri).await,
      ClientLegacyStockCommand::SpotifySetPodcastPlaybackSpeed { playback_speed } => {
        self.spotify_set_podcast_playback_speed(playback_speed).await
      }
      ClientLegacyStockCommand::SpotifySetPreset { presets } => self.spotify_set_preset(presets).await,
      ClientLegacyStockCommand::SpotifySetSaved { id, uri, saved } => self.spotify_set_saved(id, uri, saved).await,
      ClientLegacyStockCommand::SpotifySummonDj => self.spotify_summon_dj().await,
      ClientLegacyStockCommand::SpotifyPlayUri {
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

    // TODO: don't clone the message handle, do this "synchronously"?
    self.handle.state.player.request_cover_art(self.handle.clone()).await;
    Ok(())
  }

  async fn get_thumbnail_image(self, id: String) -> HandlerResult {
    tracing::debug!("({}) getting thumbnail image for id: {}", &self.handle.from, id);

    // TODO: don't clone the message handle, do this "synchronously"?
    self.handle.state.player.request_cover_art(self.handle.clone()).await;
    Ok(())
  }

  async fn get_next_tracks(&self) -> HandlerResult {
    tracing::debug!("({}) getting next tracks", &self.handle.from);
    // Ok(self.handle.respond().await?)
    Ok(())
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
        self.handle.stock_msg_id,
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
        self.handle.stock_msg_id,
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
