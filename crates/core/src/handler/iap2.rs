//! Inbound iAP2 SessionEvent router.
//!
//! Replaces the manager-internal `observe_session_events` god-function.
//! The iAP2 manager emits per-peer `Iap2Event`s upstream over a public
//! mpsc; the daemon's main loop reads from that channel and calls
//! `Iap2EventRouter::route` for each event. State mutation lives here,
//! one variant per arm.

use std::{collections::HashMap, sync::Arc};

use bluer::Address;
use bridgething_iap2::{
  SessionEvent,
  csm::now_playing::{
    MediaItemAttributes, MediaTypeKind, NowPlayingUpdate as Iap2NowPlayingUpdate, PlaybackAttributes, PlaybackState,
    RepeatMode, ShuffleMode as Iap2ShuffleMode, decode_queue_snapshot,
  },
};
use libbridgething::{
  AssetRetention, DeviceType, MediaItemUpdate, MediaType as LibMediaType, NowPlayingUpdate, PeerIap2Status,
  PlaybackUpdate, QueueItem, ShuffleMode as LibShuffleMode,
};
use tokio::sync::Mutex;

use crate::{
  bluetooth::{
    iap2::{Iap2EaGatewayHandle, Iap2Event, Iap2ReconnectHandle, StreamClosed, StreamOpened},
    profiles::ProfileMan,
  },
  state::State,
};

const IDLE_PID_HEX: &str = "0000000000000000";

/// Per-session NowPlaying context. Each iPhone reports `persistent_id`
type LastPidMap = Mutex<HashMap<Address, String>>;

#[derive(Debug, Default)]
struct QueueContext {
  expected_transfer_id: Option<u8>,
  list_avail: Option<bool>,
}
type QueueContextMap = Mutex<HashMap<Address, QueueContext>>;

#[derive(Debug, Clone)]
struct PendingArtEntry {
  transfer_id: u8,
  asset_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct Iap2PendingArt {
  inner: Arc<Mutex<HashMap<Address, PendingArtEntry>>>,
}

impl Iap2PendingArt {
  pub fn new() -> Self {
    Self::default()
  }

  async fn mark(&self, address: Address, transfer_id: u8, asset_id: String) {
    self
      .inner
      .lock()
      .await
      .insert(address, PendingArtEntry { transfer_id, asset_id });
  }

  async fn take_if_matches(&self, address: Address, transfer_id: u8) -> Option<String> {
    let mut guard = self.inner.lock().await;
    match guard.get(&address) {
      Some(entry) if entry.transfer_id == transfer_id => guard.remove(&address).map(|e| e.asset_id),
      _ => None,
    }
  }

  async fn clear(&self, address: Address) {
    self.inner.lock().await.remove(&address);
  }

  pub async fn is_pending(&self, asset_id: &str) -> bool {
    self.inner.lock().await.values().any(|e| e.asset_id == asset_id)
  }
}

#[derive(Debug)]
pub struct Iap2EventRouter {
  state: State,
  profile_man: ProfileMan,
  ea_gateway: Iap2EaGatewayHandle,
  reconnect: Iap2ReconnectHandle,
  pending_art: Iap2PendingArt,
  last_pid_hex: LastPidMap,
  queue_ctx: QueueContextMap,
}

impl Iap2EventRouter {
  pub fn new(
    state: State,
    profile_man: ProfileMan,
    ea_gateway: Iap2EaGatewayHandle,
    reconnect: Iap2ReconnectHandle,
    pending_art: Iap2PendingArt,
  ) -> Self {
    Self {
      state,
      profile_man,
      ea_gateway,
      reconnect,
      pending_art,
      last_pid_hex: Mutex::new(HashMap::new()),
      queue_ctx: Mutex::new(HashMap::new()),
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
        self.state.ancs.attach(address).await;
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
        let pid_hex = {
          let mut guard = self.last_pid_hex.lock().await;
          if let Some(pid) = update.media_item.as_ref().and_then(|m| m.persistent_id) {
            let hex = format!("{pid:016x}");
            let track_changed = guard.get(&address) != Some(&hex);
            guard.insert(address, hex.clone());
            drop(guard);
            if track_changed {
              self.pending_art.clear(address).await;
            }
            Some(hex)
          } else {
            guard.get(&address).cloned()
          }
        };
        if let Some(pid_hex) = pid_hex.as_deref()
          && pid_hex != IDLE_PID_HEX
          && let Some(transfer_id) = update.media_item.as_ref().and_then(|m| m.artwork_id)
        {
          let asset_id = format!("iap2/art/{pid_hex}/{transfer_id}");
          self.pending_art.mark(address, transfer_id, asset_id).await;
        }
        let queue_avail_change = update
          .playback
          .as_ref()
          .and_then(|p| p.queue_list_avail.map(|avail| (avail, p.queue_list_transfer_id)));
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
        if let Some((avail, transfer_id)) = queue_avail_change {
          let cleared_queue = {
            let mut guard = self.queue_ctx.lock().await;
            let entry = guard.entry(address).or_default();
            entry.list_avail = Some(avail);
            entry.expected_transfer_id = transfer_id;
            !avail
          };
          if cleared_queue {
            tracing::debug!(%address, "queue_list_avail=false; clearing iAP2 queue");
            if let Err(err) = self.state.player.apply_iap2_queue(Vec::new()).await {
              tracing::warn!(%address, ?err, "failed to clear iAP2 queue");
            }
          }
        }
      }
      SessionEvent::ArtworkBytes { transfer_id, bytes } => {
        let Some(asset_id) = self.pending_art.take_if_matches(address, transfer_id).await else {
          tracing::debug!(
            %address,
            transfer_id,
            "iAP2 artwork bytes with no matching pending entry; dropping"
          );
          return;
        };
        tracing::debug!(%address, asset_id = %asset_id, bytes = bytes.len(), "iAP2 artwork bytes -> AssetCache");
        if let Err(err) = self
          .state
          .assets
          .insert(
            asset_id.clone(),
            bytes,
            Some("image/jpeg".to_string()),
            AssetRetention::Lru,
          )
          .await
        {
          tracing::warn!(%address, ?err, "failed to insert iAP2 artwork into asset cache");
          return;
        }
        if let Err(err) = self
          .state
          .player
          .apply_artwork_id(crate::player::NowPlayingSource::Iap2, asset_id)
          .await
        {
          tracing::warn!(%address, ?err, "failed to apply iAP2 artwork id to player");
        }
      }
      SessionEvent::QueueSnapshotBytes { transfer_id, bytes } => {
        let expected = self
          .queue_ctx
          .lock()
          .await
          .get(&address)
          .and_then(|c| c.expected_transfer_id);
        if let Some(want) = expected
          && want != transfer_id
        {
          tracing::warn!(
            %address,
            transfer_id,
            expected = want,
            "iAP2 queue snapshot transfer-id mismatch; honoring arrival anyway"
          );
        }
        let raw_items = match decode_queue_snapshot(bytes) {
          Ok(items) => items,
          Err(err) => {
            tracing::warn!(%address, transfer_id, ?err, "iAP2 queue snapshot decode failed; dropping");
            return;
          }
        };
        tracing::debug!(%address, transfer_id, count = raw_items.len(), "iAP2 queue snapshot decoded");
        let queue = build_queue_items(&raw_items);
        if let Err(err) = self.state.player.apply_iap2_queue(queue).await {
          tracing::warn!(%address, ?err, "failed to apply iAP2 queue");
        }
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
        if let Err(err) = self.state.peers.set_display_name(address, update.device_name).await {
          tracing::warn!(%address, ?err, "failed to apply iAP2 device name");
        }
      }
      SessionEvent::DeviceLanguage(update) => {
        tracing::info!(%address, language = %update.language, "iAP2 device language");
        if let Err(err) = self.state.peers.set_language(address, update.language).await {
          tracing::warn!(%address, ?err, "failed to apply iAP2 device language");
        }
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
        if let Err(err) = self.state.peers.set_uuid(address, update.uuid).await {
          tracing::warn!(%address, ?err, "failed to apply iAP2 device UUID");
        }
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
        self.state.ancs.detach(address).await;
        self.last_pid_hex.lock().await.remove(&address);
        self.queue_ctx.lock().await.remove(&address);
        self.pending_art.clear(address).await;
        if let Err(err) = self.state.player.apply_iap2_queue(Vec::new()).await {
          tracing::warn!(%address, ?err, "failed to clear iAP2 queue on link-down");
        }
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
  MediaItemUpdate {
    persistent_id: pid_hex.map(|hex| format!("iap2:track:{hex}")),
    title: media.title,
    album: media.album,
    album_artist: media.album_artist,
    artist: media.artist,
    liked: media.liked,
    artwork_id: None,
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

fn build_queue_items(items: &[MediaItemAttributes]) -> Vec<QueueItem> {
  items.iter().map(build_queue_item).collect()
}

fn build_queue_item(media: &MediaItemAttributes) -> QueueItem {
  let pid_hex = media.persistent_id.map(|id| format!("{id:016x}"));
  let uri = pid_hex
    .as_deref()
    .map(|pid| format!("iap2:track:{pid}"))
    .unwrap_or_default();
  QueueItem {
    uri,
    title: media.title.clone(),
    artist: media.artist.clone().or_else(|| media.album_artist.clone()),
    album: media.album.clone(),
    artwork_id: None,
    duration_ms: media.duration_ms,
    persistent_id: pid_hex.map(|pid| format!("iap2:track:{pid}")),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn addr() -> Address {
    "AA:BB:CC:DD:EE:FF".parse().unwrap()
  }

  #[tokio::test]
  async fn pending_take_matches_transfer_id() {
    let pending = Iap2PendingArt::new();
    let id = "iap2/art/abcd/5".to_string();
    pending.mark(addr(), 5, id.clone()).await;
    assert!(pending.is_pending(&id).await);
    assert_eq!(pending.take_if_matches(addr(), 5).await, Some(id));
    assert!(!pending.is_pending("iap2/art/abcd/5").await);
  }

  #[tokio::test]
  async fn pending_rejects_mismatched_transfer_id() {
    let pending = Iap2PendingArt::new();
    pending.mark(addr(), 5, "iap2/art/abcd/5".to_string()).await;
    assert_eq!(pending.take_if_matches(addr(), 4).await, None);
    assert!(pending.is_pending("iap2/art/abcd/5").await);
  }

  #[tokio::test]
  async fn mark_replaces_prior_entry_for_same_address() {
    let pending = Iap2PendingArt::new();
    pending.mark(addr(), 5, "iap2/art/old/5".to_string()).await;
    pending.mark(addr(), 7, "iap2/art/new/7".to_string()).await;
    assert!(!pending.is_pending("iap2/art/old/5").await);
    assert!(pending.is_pending("iap2/art/new/7").await);
    assert_eq!(
      pending.take_if_matches(addr(), 7).await,
      Some("iap2/art/new/7".to_string())
    );
  }

  #[tokio::test]
  async fn clear_drops_all_for_address() {
    let pending = Iap2PendingArt::new();
    pending.mark(addr(), 5, "iap2/art/abcd/5".to_string()).await;
    pending.clear(addr()).await;
    assert!(!pending.is_pending("iap2/art/abcd/5").await);
    assert_eq!(pending.take_if_matches(addr(), 5).await, None);
  }
}
