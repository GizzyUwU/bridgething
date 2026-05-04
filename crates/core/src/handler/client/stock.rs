use std::collections::HashMap;

use base64::Engine as _;
use libbridgething::{
  ItemKind, ItemRef, QueuePosition,
  client::ClientLegacyStockCommand,
  gateway::{
    self, BridgeToGatewayLibraryMsgCommand, BridgeToGatewayPlayerMsgCommand, LibraryBrowseRequest,
    LibraryFavoritesContainsRequest,
  },
  stock::{StockPreset, StockSetPreset},
  wire::RequestError,
};

use super::{HandlerResult, MsgHandle};
use crate::stock::{
  StockConnectionType, StockInterAppSend, StockInterAppSendPayload, StockPermissionsSend, StockTip, presets,
};

const DJ_PLAYLIST_URI: &str = "spotify:playlist:37i9dQZF1EYkqdzj48dyYq";
const STOCK_BROWSE_LIMIT_MAX: u32 = 100;

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
      ClientLegacyStockCommand::SpotifyGetPlayerState => self.spotify_get_player_state().await,
      ClientLegacyStockCommand::SpotifyGetPodcast { uri, limit, offset } => {
        self.spotify_get_podcast(uri, limit, offset).await
      }
      ClientLegacyStockCommand::SpotifyGetPresets => self.spotify_get_presets().await,
      ClientLegacyStockCommand::SpotifyGetSaved { id } => self.spotify_get_saved(id).await,
      ClientLegacyStockCommand::SpotifyGetSessionState => self.spotify_get_session_state().await,
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
    self.serve_asset_to_stock(id).await
  }

  async fn get_thumbnail_image(self, id: String) -> HandlerResult {
    self.serve_asset_to_stock(id).await
  }

  async fn serve_asset_to_stock(self, id: String) -> HandlerResult {
    tracing::debug!("({}) stock image lookup for id: {}", &self.handle.from, id);
    let stock_msg_id = self.handle.stock_msg_id;
    match self.handle.state.assets.get(&id).await? {
      Some(asset) => {
        let image_data = base64::engine::general_purpose::STANDARD.encode(&asset.bytes);
        self
          .handle
          .send_stock(StockInterAppSend::new(
            stock_msg_id,
            StockInterAppSendPayload::Image {
              height: 0,
              width: 0,
              image_data,
            },
          ))
          .await?;
      }
      None => {
        tracing::trace!("({}) asset miss for stock image: {}", &self.handle.from, id);
        self
          .handle
          .send_stock(StockInterAppSend::make_ack(stock_msg_id))
          .await?;
      }
    }
    Ok(())
  }

  async fn get_next_tracks(&self) -> HandlerResult {
    let reply = self.handle.state.player.queue_reply().await;
    let payload = crate::stock::interapp::player_queue_to_stock(reply);
    self
      .handle
      .send_stock(StockInterAppSend::new(self.handle.stock_msg_id, payload))
      .await?;
    Ok(())
  }

  async fn spotify_get_children(&self, parent_id: String, limit: usize, offset: Option<usize>) -> HandlerResult {
    self
      .browse_through_modern(Some(parent_id), limit_to_u32(limit), offset_to_u32(offset))
      .await
  }

  async fn spotify_get_home(&self, limit: usize, _limit_overrides: HashMap<String, usize>) -> HandlerResult {
    self.browse_through_modern(None, limit_to_u32(limit), 0).await
  }

  async fn spotify_get_podcast(&self, uri: String, limit: Option<usize>, offset: Option<usize>) -> HandlerResult {
    let limit = limit.map(limit_to_u32).unwrap_or(STOCK_BROWSE_LIMIT_MAX);
    self
      .browse_through_modern(Some(uri), limit, offset_to_u32(offset))
      .await
  }

  async fn browse_through_modern(&self, node_id: Option<String>, limit: u32, offset: u32) -> HandlerResult {
    if !self.has_gateway() {
      return self.send_empty_children(limit, offset).await;
    }
    let req = LibraryBrowseRequest {
      node_id,
      limit: limit.min(STOCK_BROWSE_LIMIT_MAX),
      offset,
    };
    match self.handle.bluetooth.gateway_man.request_bulk(None, req).await {
      Ok(reply) => {
        let payload = crate::stock::interapp::library_browse_to_stock(reply.result, limit, offset);
        self
          .handle
          .send_stock(StockInterAppSend::new(self.handle.stock_msg_id, payload))
          .await?;
      }
      Err(err) => {
        log_request_failure("library.browse", &err);
        self.send_empty_children(limit, offset).await?;
      }
    }
    Ok(())
  }

  async fn send_empty_children(&self, limit: u32, offset: u32) -> HandlerResult {
    self
      .handle
      .send_stock(StockInterAppSend::new(
        self.handle.stock_msg_id,
        StockInterAppSendPayload::ItemChildren {
          limit: limit as usize,
          offset: offset as usize,
          total: offset as usize,
          items: Vec::new(),
        },
      ))
      .await?;
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

  async fn spotify_get_player_state(&self) -> HandlerResult {
    let reply = self.handle.state.player.state_reply().await;
    let payload = crate::stock::interapp::player_state_to_stock(reply);
    self
      .handle
      .send_stock(StockInterAppSend::new(self.handle.stock_msg_id, payload))
      .await?;
    Ok(())
  }

  async fn spotify_get_session_state(&self) -> HandlerResult {
    let snapshot = self.handle.state.capabilities.snapshot();
    let connection_type = match snapshot.network.kind {
      libbridgething::NetworkKind::Wifi | libbridgething::NetworkKind::Ethernet => StockConnectionType::Wlan,
      libbridgething::NetworkKind::Cellular => StockConnectionType::FourG,
      libbridgething::NetworkKind::Unknown => {
        if snapshot.gateway.is_some() {
          StockConnectionType::Wlan
        } else {
          StockConnectionType::None
        }
      }
    };
    self
      .handle
      .send_stock(StockInterAppSend::new(
        self.handle.stock_msg_id,
        StockInterAppSendPayload::SessionState {
          connection_type,
          is_in_forced_offline_mode: false,
          is_logged_in: snapshot.gateway.is_some(),
          is_offline: snapshot.gateway.is_none(),
        },
      ))
      .await?;
    Ok(())
  }

  async fn spotify_get_presets(&self) -> HandlerResult {
    let result = match presets::list(&self.handle.state.kv).await {
      Ok(list) => list,
      Err(err) => {
        tracing::warn!(?err, "stock get_presets read failed");
        Vec::new()
      }
    };
    self
      .handle
      .send_stock(StockInterAppSend::new(
        self.handle.stock_msg_id,
        StockInterAppSendPayload::Presets { result, success: true },
      ))
      .await?;
    Ok(())
  }

  async fn spotify_get_saved(&self, id: String) -> HandlerResult {
    if !self.has_gateway() {
      return self.send_saved_result(false).await;
    }
    let req = LibraryFavoritesContainsRequest { uris: vec![id] };
    match self.handle.bluetooth.gateway_man.request_bulk(None, req).await {
      Ok(reply) => {
        let liked = reply.liked.first().copied().unwrap_or(false);
        self.send_saved_result(liked).await?;
      }
      Err(err) => {
        log_request_failure("library.favoritesContains", &err);
        self.send_saved_result(false).await?;
      }
    }
    Ok(())
  }

  async fn send_saved_result(&self, liked: bool) -> HandlerResult {
    self
      .handle
      .send_stock(StockInterAppSend::new(
        self.handle.stock_msg_id,
        StockInterAppSendPayload::Saved { saved: liked },
      ))
      .await?;
    Ok(())
  }

  async fn spotify_get_tips(&self) -> HandlerResult {
    self
      .handle
      .send_stock(StockInterAppSend::new(
        self.handle.stock_msg_id,
        StockInterAppSendPayload::Tips { result: canned_tips() },
      ))
      .await?;
    Ok(())
  }

  async fn spotify_get_tts(&self, _file: String) -> HandlerResult {
    // Stock TTS played pre-cached audio files on the phone (closer to Earcon
    // than modern Audio.tts text). The daemon has no companion path to
    // request such a file; ack to resolve the webapp's promise.
    self.ack().await
  }

  async fn spotify_play_podcast_trailer(&self, uri: String) -> HandlerResult {
    self.forward_play(uri).await
  }

  async fn spotify_queue_uri(&self, uri: String) -> HandlerResult {
    if !self.has_gateway() {
      return self.ack().await;
    }
    self
      .handle
      .bluetooth
      .gateway_man
      .broadcast_command(BridgeToGatewayPlayerMsgCommand::Queue(gateway::QueueUri {
        uri,
        position: QueuePosition::Append,
      }))
      .await;
    self.ack().await
  }

  async fn spotify_set_podcast_playback_speed(&self, playback_speed: usize) -> HandlerResult {
    if !self.has_gateway() {
      return self.ack().await;
    }
    self
      .handle
      .bluetooth
      .gateway_man
      .broadcast_command(BridgeToGatewayPlayerMsgCommand::SetSpeed(gateway::SetSpeed {
        speed: playback_speed as f32 / 100.0,
      }))
      .await;
    self.ack().await
  }

  async fn spotify_set_preset(&self, requests: Vec<StockSetPreset>) -> HandlerResult {
    for req in requests {
      let preset = StockPreset {
        context_uri: req.context_uri,
        image_url: None,
        slot_index: req.slot_index,
        name: None,
        description: None,
      };
      if let Err(err) = presets::upsert(&self.handle.state.kv, &preset).await {
        tracing::warn!(?err, "stock set_preset write failed");
      }
    }
    let result = presets::list(&self.handle.state.kv).await.unwrap_or_default();
    self
      .handle
      .send_stock(StockInterAppSend::new(
        self.handle.stock_msg_id,
        StockInterAppSendPayload::Presets { result, success: true },
      ))
      .await?;
    Ok(())
  }

  async fn spotify_set_saved(&self, id: Option<String>, uri: Option<String>, saved: bool) -> HandlerResult {
    if let Some(item_uri) = uri.or(id)
      && self.has_gateway() {
        self
          .handle
          .bluetooth
          .gateway_man
          .broadcast_command(BridgeToGatewayLibraryMsgCommand::FavoritesSet(gateway::FavoritesSet {
            item: ItemRef {
              uri: item_uri,
              kind: ItemKind::Track,
              persistent_id: None,
            },
            liked: saved,
          }))
          .await;
      }
    self.ack().await
  }

  async fn spotify_summon_dj(&self) -> HandlerResult {
    self.forward_play(DJ_PLAYLIST_URI.to_string()).await
  }

  async fn spotify_play_uri(
    &self,
    uri: String,
    _feature_identifier: String,
    _interaction_id: Option<String>,
    _skip_to_uri: Option<String>,
    _skip_to_uid: Option<String>,
  ) -> HandlerResult {
    self.forward_play(uri).await
  }

  async fn forward_play(&self, uri: String) -> HandlerResult {
    if !self.has_gateway() {
      return self.ack().await;
    }
    self
      .handle
      .bluetooth
      .gateway_man
      .broadcast_command(BridgeToGatewayPlayerMsgCommand::Play(gateway::PlayUri {
        uri,
        context: None,
      }))
      .await;
    self.ack().await
  }

  fn has_gateway(&self) -> bool {
    self.handle.state.capabilities.snapshot().gateway.is_some()
  }

  async fn ack(&self) -> HandlerResult {
    self
      .handle
      .send_stock(StockInterAppSend::make_ack(self.handle.stock_msg_id))
      .await?;
    Ok(())
  }
}

fn limit_to_u32(limit: usize) -> u32 {
  u32::try_from(limit).unwrap_or(STOCK_BROWSE_LIMIT_MAX)
}

fn offset_to_u32(offset: Option<usize>) -> u32 {
  offset.and_then(|o| u32::try_from(o).ok()).unwrap_or(0)
}

fn log_request_failure(verb: &str, err: &RequestError<gateway::LibraryErrorReply>) {
  match err {
    RequestError::Domain(domain) => {
      tracing::debug!(?domain.error, "{verb} returned domain error; sending stock fallback");
    }
    RequestError::Protocol(err) => {
      tracing::warn!(?err, "{verb} protocol error; sending stock fallback");
    }
    RequestError::ResponseMismatch => {
      tracing::error!("{verb} response did not match expected shape; sending stock fallback");
    }
  }
}

fn canned_tips() -> Vec<StockTip> {
  vec![
    StockTip {
      id: 1,
      title: "running on bridgething".into(),
      description: "this car thing is alive thanks to thinglabs.".into(),
      action: "".into(),
    },
    StockTip {
      id: 2,
      title: "no spotify required".into(),
      description: "your phone's the boss now. spotify, apple music, whatever you connect.".into(),
      action: "".into(),
    },
    StockTip {
      id: 3,
      title: "press the wheel to chill".into(),
      description: "single click pauses, double-click skips. classic car thing moves.".into(),
      action: "".into(),
    },
  ]
}
