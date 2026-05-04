//! Inbound iAP2 SessionEvent router.
//!
//! Replaces the manager-internal `observe_session_events` god-function.
//! The iAP2 manager emits per-peer `Iap2Event`s upstream over a public
//! mpsc; the daemon's main loop reads from that channel and calls
//! `Iap2EventRouter::route` for each event. State mutation lives here,
//! one variant per arm.

use std::collections::HashMap;

use bluer::Address;
use bridgething_iap2::{
  SessionEvent,
  csm::now_playing::{
    MediaItemAttributes, MediaTypeKind, NowPlayingUpdate as Iap2NowPlayingUpdate, PlaybackAttributes, PlaybackState,
    RepeatMode, ShuffleMode as Iap2ShuffleMode,
  },
};
use libbridgething::{
  AssetRetention, DeviceType, MediaItemUpdate, MediaType as LibMediaType, NowPlayingUpdate, PeerIap2Status,
  PlaybackUpdate, ShuffleMode as LibShuffleMode,
};
use tokio::sync::Mutex;

use crate::{
  bluetooth::{
    iap2::{Iap2EaGatewayHandle, Iap2Event, Iap2ReconnectHandle, StreamClosed, StreamOpened},
    profiles::ProfileMan,
  },
  state::State,
};

/// Per-session NowPlaying context. Each iPhone reports `persistent_id`
/// only when it changes; subsequent deltas (for example, an artwork
/// update on the same track) reuse the prior value. We hold the most
/// recent hex form per peer to synthesise asset ids and tag inbound
/// FileTransfer bytes with their owning track.
type LastPidMap = Mutex<HashMap<Address, String>>;

#[derive(Debug)]
pub struct Iap2EventRouter {
  state: State,
  profile_man: ProfileMan,
  ea_gateway: Iap2EaGatewayHandle,
  reconnect: Iap2ReconnectHandle,
  last_pid_hex: LastPidMap,
}

impl Iap2EventRouter {
  pub fn new(
    state: State,
    profile_man: ProfileMan,
    ea_gateway: Iap2EaGatewayHandle,
    reconnect: Iap2ReconnectHandle,
  ) -> Self {
    Self {
      state,
      profile_man,
      ea_gateway,
      reconnect,
      last_pid_hex: Mutex::new(HashMap::new()),
    }
  }

  pub async fn route(&self, event: Iap2Event) {
    let Iap2Event { address, event } = event;
    match event {
      SessionEvent::LinkEstablished(lsp) => {
        tracing::info!(
          %address,
          peer_max_outgoing = lsp.max_outgoing,
          peer_max_len = lsp.max_len,
          "iAP2 link Established",
        );
        if let Err(err) = self.profile_man.upsert_paired_device(address, DeviceType::Ios).await {
          tracing::warn!(%address, ?err, "failed to upsert peer for iAP2 link");
        }
        let _ = self.state.peers.set_iap2(address, PeerIap2Status::LinkUp).await;
      }
      SessionEvent::Authenticated => {
        tracing::info!(%address, "iAP2 authenticated");
        let _ = self.state.peers.set_iap2(address, PeerIap2Status::Authenticated).await;
      }
      SessionEvent::Identified => {
        tracing::info!(%address, "iAP2 identified");
        let _ = self.state.peers.set_iap2(address, PeerIap2Status::Identified).await;
      }
      SessionEvent::AuthFailed => tracing::warn!(%address, "iAP2 auth failed"),
      SessionEvent::IdentificationRejected { rejected_params } => {
        tracing::warn!(%address, ?rejected_params, "iAP2 identification rejected");
      }
      SessionEvent::NowPlayingUpdate(update) => {
        let pid_hex = if let Some(pid) = update.media_item.as_ref().and_then(|m| m.persistent_id) {
          let hex = format!("{pid:016x}");
          self.last_pid_hex.lock().await.insert(address, hex.clone());
          Some(hex)
        } else {
          self.last_pid_hex.lock().await.get(&address).cloned()
        };
        let lib_update = translate_now_playing(update, pid_hex.as_deref());
        tracing::debug!(%address, ?lib_update, "iAP2 now-playing delta");
        if let Err(err) = self
          .state
          .player
          .apply_now_playing(crate::player::NowPlayingSource::Iap2, lib_update)
          .await
        {
          tracing::warn!(%address, ?err, "failed to apply iAP2 now-playing delta");
        }
      }
      SessionEvent::ArtworkBytes { transfer_id, bytes } => {
        let pid_hex = self.last_pid_hex.lock().await.get(&address).cloned();
        let Some(pid_hex) = pid_hex else {
          tracing::warn!(%address, transfer_id, "iAP2 artwork bytes received before any NowPlayingUpdate; dropping");
          return;
        };
        let id = format!("iap2/art/{pid_hex}/{transfer_id}");
        tracing::debug!(%address, asset_id = %id, bytes = bytes.len(), "iAP2 artwork bytes -> AssetCache");
        if let Err(err) = self
          .state
          .assets
          .insert(
            id,
            tokio_util::bytes::Bytes::copy_from_slice(&bytes),
            Some("image/jpeg".to_string()),
            AssetRetention::Lru,
          )
          .await
        {
          tracing::warn!(%address, ?err, "failed to insert iAP2 artwork into asset cache");
        }
      }
      SessionEvent::QueueSnapshotBytes { transfer_id, bytes } => {
        tracing::debug!(
          %address,
          transfer_id,
          bytes = bytes.len(),
          "iAP2 queue snapshot bytes received (parser TBD)"
        );
      }
      SessionEvent::CallStateUpdate(update) => {
        tracing::debug!(%address, ?update, "iAP2 call-state update");
        if let Err(err) = self.state.telephony.apply_iap2_call_state(update).await {
          tracing::warn!(%address, ?err, "failed to apply iAP2 call-state update");
        }
      }
      SessionEvent::CommunicationsUpdate(update) => {
        tracing::debug!(%address, ?update, "iAP2 communications update");
        if let Err(err) = self.state.telephony.apply_iap2_communications(update).await {
          tracing::warn!(%address, ?err, "failed to apply iAP2 communications update");
        }
      }
      SessionEvent::DeviceName(update) => {
        tracing::info!(%address, name = %update.device_name, "iAP2 device name");
      }
      SessionEvent::DeviceLanguage(update) => {
        tracing::info!(%address, language = %update.language, "iAP2 device language");
      }
      SessionEvent::DeviceTime(update) => {
        tracing::info!(
          %address,
          unix_s = update.seconds_since_reference_date,
          tz_offset_minutes = update.tz_offset_minutes,
          dst_offset_minutes = update.dst_offset_minutes,
          "iAP2 device time"
        );
        if let Err(err) = self
          .state
          .time
          .apply_iap2_update(
            update.seconds_since_reference_date,
            update.tz_offset_minutes,
            update.dst_offset_minutes,
          )
          .await
        {
          tracing::warn!(%address, ?err, "failed to apply iAP2 time update");
        }
      }
      SessionEvent::DeviceUuid(update) => {
        tracing::info!(%address, uuid = %update.uuid, "iAP2 device UUID");
      }
      SessionEvent::EaStreamOpened {
        stream_id,
        protocol_id,
        inbound_rx,
        outbound,
      } => {
        tracing::info!(%address, stream_id, protocol_id, "iAP2 EA stream opened");
        self
          .ea_gateway
          .notify_open(StreamOpened {
            address,
            stream_id,
            protocol_id,
            inbound_rx,
            outbound,
          })
          .await;
      }
      SessionEvent::EaStreamClosed { stream_id } => {
        tracing::info!(%address, stream_id, "iAP2 EA stream closed");
        self.ea_gateway.notify_closed(StreamClosed { address, stream_id }).await;
      }
      SessionEvent::LinkDown(reason) => {
        tracing::info!(%address, %reason, "iAP2 link down");
        let _ = self.state.peers.set_iap2(address, PeerIap2Status::None).await;
        self.last_pid_hex.lock().await.remove(&address);
        self.reconnect.kick(address).await;
      }
    }
  }
}

fn translate_now_playing(update: Iap2NowPlayingUpdate, persistent_hex: Option<&str>) -> NowPlayingUpdate {
  NowPlayingUpdate {
    media_item: update.media_item.map(|m| translate_media_item(m, persistent_hex)),
    playback: update.playback.map(translate_playback),
  }
}

fn translate_media_item(media: MediaItemAttributes, persistent_hex: Option<&str>) -> MediaItemUpdate {
  let pid_hex = media
    .persistent_id
    .map(|id| format!("{id:016x}"))
    .or_else(|| persistent_hex.map(str::to_string));
  let artwork_id = match (media.artwork_id, pid_hex.as_deref()) {
    (Some(id), Some(pid)) => Some(format!("iap2/art/{pid}/{id}")),
    _ => None,
  };
  MediaItemUpdate {
    persistent_id: pid_hex.map(|hex| format!("iap2:track:{hex}")),
    title: media.title,
    album: media.album,
    album_artist: media.album_artist,
    artist: media.artist,
    liked: media.liked,
    artwork_id,
    duration_ms: media.duration_ms,
    media_types: media.media_types.map(translate_media_types),
    track_number: media.track_number,
    track_count: media.track_count,
    is_like_supported: media.like_supported,
    is_ban_supported: media.ban_supported,
    is_banned: media.banned,
    is_resident_on_device: media.resident_on_device,
    chapter_count: media.chapter_count,
  }
}

fn translate_playback(play: PlaybackAttributes) -> PlaybackUpdate {
  PlaybackUpdate {
    playing: play.state.map(|s| matches!(s, PlaybackState::Playing)),
    position_ms: play.position_ms,
    shuffle: play.shuffle_mode.map(|m| m.is_on()),
    shuffle_mode: play.shuffle_mode.map(translate_shuffle),
    repeat: play.repeat.map(translate_repeat),
    app_bundle: play.app_bundle,
    app_display_name: play.app_display_name,
    queue_index: play.queue_index,
    queue_count: play.queue_count,
    queue_chapter_index: play.queue_chapter_index,
    playback_speed: play.playback_speed_hundredths.map(|h| f32::from(h) / 100.0),
    set_elapsed_time_available: play.set_elapsed_time_available,
    queue_list_avail: play.queue_list_avail,
    apple_music_radio_ad: play.apple_music_radio_ad,
    apple_music_radio_station_name: play.apple_music_radio_station_name,
  }
}

fn translate_repeat(mode: RepeatMode) -> libbridgething::RepeatMode {
  match mode {
    RepeatMode::Off => libbridgething::RepeatMode::Off,
    RepeatMode::Track => libbridgething::RepeatMode::One,
    RepeatMode::All => libbridgething::RepeatMode::All,
  }
}

fn translate_shuffle(mode: Iap2ShuffleMode) -> LibShuffleMode {
  match mode {
    Iap2ShuffleMode::Off => LibShuffleMode::Off,
    Iap2ShuffleMode::Songs => LibShuffleMode::Songs,
    Iap2ShuffleMode::Albums => LibShuffleMode::Albums,
  }
}

fn translate_media_types(types: Vec<MediaTypeKind>) -> Vec<LibMediaType> {
  types
    .into_iter()
    .map(|t| match t {
      MediaTypeKind::Music => LibMediaType::Music,
      MediaTypeKind::Podcast => LibMediaType::Podcast,
      MediaTypeKind::AudioBook => LibMediaType::AudioBook,
    })
    .collect()
}
