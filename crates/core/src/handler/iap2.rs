//! Inbound iAP2 SessionEvent router.
//!
//! The iAP2 manager emits per-peer `Iap2Event`s upstream over a public
//! mpsc; the daemon's main loop reads from that channel and calls
//! `Iap2EventRouter::route` for each event. State mutation lives here,
//! one variant per arm.

use std::{
  collections::HashMap,
  sync::Arc,
  time::{Duration, Instant},
};

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
  gateway::{BridgeToGatewayPlayerMsgEvent, PlaybackHint},
};
use tokio::sync::Mutex;

use crate::{
  bluetooth::{
    BluetoothMan,
    iap2::{Iap2EaGatewayHandle, Iap2Event, Iap2ReconnectHandle, StreamClosed, StreamOpened},
  },
  state::State,
};

const IDLE_PID_HEX: &str = "0000000000000000";
const NONMUSIC_PREFIX: &str = "nonmusic-";
const HINT_DEBOUNCE: Duration = Duration::from_millis(250);

#[derive(Debug, Default, Clone)]
struct HintCheckpoint {
  track_pid_hex: Option<String>,
  emitted_pid: Option<String>,
  playing: Option<bool>,
  app_bundle: Option<String>,
  duration_ms: Option<u32>,
  last_emit: Option<Instant>,
}

type HintStateMap = Mutex<HashMap<Address, HintCheckpoint>>;

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
  bluetooth: BluetoothMan,
  ea_gateway: Iap2EaGatewayHandle,
  reconnect: Iap2ReconnectHandle,
  pending_art: Iap2PendingArt,
  hint_state: HintStateMap,
  queue_ctx: QueueContextMap,
}

impl Iap2EventRouter {
  pub fn new(
    state: State,
    bluetooth: BluetoothMan,
    ea_gateway: Iap2EaGatewayHandle,
    reconnect: Iap2ReconnectHandle,
    pending_art: Iap2PendingArt,
  ) -> Self {
    Self {
      state,
      bluetooth,
      ea_gateway,
      reconnect,
      pending_art,
      hint_state: Mutex::new(HashMap::new()),
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
        if let Some(profile_man) = self.bluetooth.profile_man.try_get()
          && let Err(err) = profile_man.upsert_paired_device(address, DeviceType::Ios).await
        {
          tracing::warn!(%address, ?err, "failed to upsert peer for iAP2 link");
        }
        let _ = self.state.peers.set_iap2(address, PeerIap2Status::LinkUp).await;
        self.bluetooth.le.attach(address).await;
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
          let mut guard = self.hint_state.lock().await;
          let entry = guard.entry(address).or_default();
          if let Some(key) = delta_track_key(update.media_item.as_ref()) {
            let track_changed = entry.track_pid_hex.as_deref() != Some(&key);
            entry.track_pid_hex = Some(key.clone());
            drop(guard);
            if track_changed {
              self.pending_art.clear(address).await;
            }
            Some(key)
          } else {
            entry.track_pid_hex.clone()
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
        let hint = self.evaluate_hint(address, &lib_update).await;
        if let Err(err) = self
          .state
          .player
          .apply_now_playing(crate::player::NowPlayingSource::Iap2, lib_update)
          .await
        {
          tracing::warn!(%address, ?err, "failed to apply iAP2 now-playing delta");
        }
        if let Some(hint) = hint {
          tracing::debug!(%address, ?hint, "emitting iAP2 playback hint");
          self
            .bluetooth
            .gateway_man
            .broadcast(BridgeToGatewayPlayerMsgEvent::Hint(hint))
            .await;
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
        self.state.peers.set_display_name(address, update.device_name).await;
      }
      SessionEvent::DeviceLanguage(update) => {
        tracing::info!(%address, language = %update.language, "iAP2 device language");
        self.state.peers.set_language(address, update.language).await;
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
        self.state.peers.set_uuid(address, update.uuid).await;
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
        self.bluetooth.le.detach(address).await;
        self.hint_state.lock().await.remove(&address);
        self.queue_ctx.lock().await.remove(&address);
        self.pending_art.clear(address).await;
        if let Err(err) = self.state.player.apply_iap2_queue(Vec::new()).await {
          tracing::warn!(%address, ?err, "failed to clear iAP2 queue on link-down");
        }
        self.reconnect.kick(address).await;
      }
    }
  }

  async fn evaluate_hint(&self, address: Address, update: &NowPlayingUpdate) -> Option<PlaybackHint> {
    let mut guard = self.hint_state.lock().await;
    let entry = guard.entry(address).or_default();
    evaluate_hint_against(entry, update, Instant::now())
  }
}

fn evaluate_hint_against(entry: &mut HintCheckpoint, update: &NowPlayingUpdate, now: Instant) -> Option<PlaybackHint> {
  let incoming_pid = update.media_item.as_ref().and_then(|m| m.persistent_id.clone());
  if let Some(pid) = incoming_pid.as_deref()
    && pid_is_idle(pid)
  {
    return None;
  }

  let incoming_playing = update.playback.as_ref().and_then(|p| p.playing);
  let incoming_bundle = update.playback.as_ref().and_then(|p| p.app_bundle.clone());
  let incoming_duration = update.media_item.as_ref().and_then(|m| m.duration_ms);

  let pid_for_emit = incoming_pid.clone().or_else(|| entry.emitted_pid.clone());
  let playing_for_emit = incoming_playing.or(entry.playing);
  let bundle_for_emit = incoming_bundle.clone().or_else(|| entry.app_bundle.clone());
  let duration_for_emit = incoming_duration.or(entry.duration_ms);

  let pid_changed = incoming_pid.is_some() && entry.emitted_pid != incoming_pid;
  let playing_changed = incoming_playing.is_some() && incoming_playing != entry.playing;
  let bundle_changed = incoming_bundle.is_some() && incoming_bundle != entry.app_bundle;

  if !(pid_changed || playing_changed || bundle_changed) {
    return None;
  }
  if let Some(last) = entry.last_emit
    && now.duration_since(last) < HINT_DEBOUNCE
  {
    return None;
  }

  entry.emitted_pid = pid_for_emit.clone();
  entry.playing = playing_for_emit;
  entry.app_bundle = bundle_for_emit.clone();
  entry.duration_ms = duration_for_emit;
  entry.last_emit = Some(now);

  Some(PlaybackHint {
    app_bundle: bundle_for_emit,
    persistent_id: pid_for_emit,
    playing: playing_for_emit,
    duration_ms: duration_for_emit,
  })
}

fn pid_is_idle(pid: &str) -> bool {
  pid == IDLE_PID_HEX || pid.ends_with(&format!(":{IDLE_PID_HEX}"))
}

fn delta_track_key(media: Option<&MediaItemAttributes>) -> Option<String> {
  let media = media?;
  let title = media.title.as_deref().filter(|t| !t.is_empty());
  match media.persistent_id {
    Some(pid) if pid != 0 => Some(format!("{pid:016x}")),
    _ if title.is_some() => Some(nonmusic_key(title.unwrap(), media.artist.as_deref())),
    Some(0) => Some(IDLE_PID_HEX.to_string()),
    _ => None,
  }
}

// pid-less tracks (Spotify-on-iOS, non-music apps) collide on the literal "nonmusic" slot
// because iAP2's transfer-id is u8 and reused. Hash a stable content fingerprint instead.
fn nonmusic_key(title: &str, artist: Option<&str>) -> String {
  use std::hash::{DefaultHasher, Hash, Hasher};
  let mut hasher = DefaultHasher::new();
  title.hash(&mut hasher);
  if let Some(a) = artist {
    a.hash(&mut hasher);
  }
  format!("{NONMUSIC_PREFIX}{:016x}", hasher.finish())
}

fn translate_now_playing(update: Iap2NowPlayingUpdate, track_key: Option<&str>) -> NowPlayingUpdate {
  NowPlayingUpdate {
    media_item: update.media_item.map(|m| translate_media_item(m, track_key)),
    playback: update.playback.map(translate_playback),
  }
}

fn translate_media_item(media: MediaItemAttributes, track_key: Option<&str>) -> MediaItemUpdate {
  MediaItemUpdate {
    persistent_id: track_key.map(|key| format!("iap2:track:{key}")),
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

  fn track_update(pid_hex: &str, playing: bool, app_bundle: &str) -> NowPlayingUpdate {
    NowPlayingUpdate {
      media_item: Some(MediaItemUpdate {
        persistent_id: Some(format!("iap2:track:{pid_hex}")),
        title: None,
        album: None,
        album_artist: None,
        artist: None,
        liked: None,
        artwork_id: None,
        duration_ms: Some(180_000),
        media_types: None,
        track_number: None,
        track_count: None,
        is_like_supported: None,
        is_ban_supported: None,
        is_banned: None,
        is_resident_on_device: None,
        chapter_count: None,
      }),
      playback: Some(PlaybackUpdate {
        playing: Some(playing),
        position_ms: None,
        shuffle: None,
        shuffle_mode: None,
        repeat: None,
        app_bundle: Some(app_bundle.to_string()),
        app_display_name: None,
        queue_index: None,
        queue_count: None,
        queue_chapter_index: None,
        playback_speed: None,
        set_elapsed_time_available: None,
        queue_list_avail: None,
        apple_music_radio_ad: None,
        apple_music_radio_station_name: None,
      }),
    }
  }

  fn playback_only(playing: bool) -> NowPlayingUpdate {
    NowPlayingUpdate {
      media_item: None,
      playback: Some(PlaybackUpdate {
        playing: Some(playing),
        position_ms: None,
        shuffle: None,
        shuffle_mode: None,
        repeat: None,
        app_bundle: None,
        app_display_name: None,
        queue_index: None,
        queue_count: None,
        queue_chapter_index: None,
        playback_speed: None,
        set_elapsed_time_available: None,
        queue_list_avail: None,
        apple_music_radio_ad: None,
        apple_music_radio_station_name: None,
      }),
    }
  }

  fn bundle_only(app_bundle: &str) -> NowPlayingUpdate {
    NowPlayingUpdate {
      media_item: None,
      playback: Some(PlaybackUpdate {
        playing: None,
        position_ms: None,
        shuffle: None,
        shuffle_mode: None,
        repeat: None,
        app_bundle: Some(app_bundle.to_string()),
        app_display_name: None,
        queue_index: None,
        queue_count: None,
        queue_chapter_index: None,
        playback_speed: None,
        set_elapsed_time_available: None,
        queue_list_avail: None,
        apple_music_radio_ad: None,
        apple_music_radio_station_name: None,
      }),
    }
  }

  #[test]
  fn track_change_emits_hint() {
    let mut entry = HintCheckpoint::default();
    let now = Instant::now();
    let hint = evaluate_hint_against(&mut entry, &track_update("aa", true, "com.spotify.client"), now)
      .expect("track change emits hint");
    assert_eq!(hint.persistent_id.as_deref(), Some("iap2:track:aa"));
    assert_eq!(hint.playing, Some(true));
    assert_eq!(hint.app_bundle.as_deref(), Some("com.spotify.client"));
    assert_eq!(hint.duration_ms, Some(180_000));
  }

  #[test]
  fn play_state_flip_emits_hint() {
    let mut entry = HintCheckpoint::default();
    let t0 = Instant::now();
    evaluate_hint_against(&mut entry, &track_update("aa", true, "com.spotify.client"), t0).unwrap();
    let t1 = t0 + HINT_DEBOUNCE;
    let hint = evaluate_hint_against(&mut entry, &playback_only(false), t1).expect("flip emits hint");
    assert_eq!(hint.playing, Some(false));
    assert_eq!(hint.persistent_id.as_deref(), Some("iap2:track:aa"));
  }

  #[test]
  fn app_bundle_change_emits_hint() {
    let mut entry = HintCheckpoint::default();
    let t0 = Instant::now();
    evaluate_hint_against(&mut entry, &track_update("aa", true, "com.spotify.client"), t0).unwrap();
    let t1 = t0 + HINT_DEBOUNCE;
    let hint = evaluate_hint_against(&mut entry, &bundle_only("com.apple.Music"), t1).expect("bundle emits hint");
    assert_eq!(hint.app_bundle.as_deref(), Some("com.apple.Music"));
  }

  #[test]
  fn debounce_collapses_back_to_back_changes() {
    let mut entry = HintCheckpoint::default();
    let t0 = Instant::now();
    let first =
      evaluate_hint_against(&mut entry, &track_update("aa", true, "com.spotify.client"), t0).expect("first emits");
    assert_eq!(first.persistent_id.as_deref(), Some("iap2:track:aa"));

    // inside the debounce window the emit is suppressed but the checkpoint stays at the announced state,
    // so the next post-window delta still notices the change.
    let t1 = t0 + Duration::from_millis(100);
    assert!(evaluate_hint_against(&mut entry, &track_update("bb", true, "com.spotify.client"), t1).is_none());
    assert_eq!(entry.emitted_pid.as_deref(), Some("iap2:track:aa"));

    let t2 = t0 + Duration::from_millis(300);
    let post = evaluate_hint_against(&mut entry, &track_update("bb", true, "com.spotify.client"), t2)
      .expect("post-window emits");
    assert_eq!(post.persistent_id.as_deref(), Some("iap2:track:bb"));
  }

  #[test]
  fn idle_pid_suppresses_emit() {
    let mut entry = HintCheckpoint::default();
    assert!(
      evaluate_hint_against(
        &mut entry,
        &track_update(IDLE_PID_HEX, false, "com.spotify.client"),
        Instant::now()
      )
      .is_none()
    );
    assert!(entry.emitted_pid.is_none());
  }

  #[test]
  fn no_change_no_emit() {
    let mut entry = HintCheckpoint::default();
    let t0 = Instant::now();
    evaluate_hint_against(&mut entry, &track_update("aa", true, "com.spotify.client"), t0).unwrap();
    let t1 = t0 + HINT_DEBOUNCE;
    assert!(
      evaluate_hint_against(&mut entry, &track_update("aa", true, "com.spotify.client"), t1).is_none(),
      "identical state must not re-emit"
    );
  }

  #[test]
  fn position_only_delta_does_not_emit() {
    let mut entry = HintCheckpoint::default();
    let t0 = Instant::now();
    evaluate_hint_against(&mut entry, &track_update("aa", true, "com.spotify.client"), t0).unwrap();
    let t1 = t0 + HINT_DEBOUNCE;
    let position_delta = NowPlayingUpdate {
      media_item: None,
      playback: Some(PlaybackUpdate {
        playing: None,
        position_ms: Some(45_000),
        shuffle: None,
        shuffle_mode: None,
        repeat: None,
        app_bundle: None,
        app_display_name: None,
        queue_index: None,
        queue_count: None,
        queue_chapter_index: None,
        playback_speed: None,
        set_elapsed_time_available: None,
        queue_list_avail: None,
        apple_music_radio_ad: None,
        apple_music_radio_station_name: None,
      }),
    };
    assert!(evaluate_hint_against(&mut entry, &position_delta, t1).is_none());
  }
}
