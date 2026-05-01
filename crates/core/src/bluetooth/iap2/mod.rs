//! iAP2 over RFCOMM. Sibling to `rfcomm/` (Android-native gateway) and
//! `ble/` (legacy GATT gateway). Registers the iAP2 accessory profile,
//! accepts iPhone connect requests, and spawns one [`Iap2Session`] per
//! active link. MFi chip access is required; initialization probes the
//! chip first and skips profile registration entirely if the probe
//! fails (Car Things without working MFi silicon still get a usable
//! daemon, just no iOS support).
//!
//! Session events (Authenticated, Identified, LinkDown, ...) are
//! observability-only at this layer - logged and dropped. Higher-layer
//! wedges (NowPlaying state, HID transport bindings, EA dispatch) plug
//! their own typed event surfaces in at the `observe_session_events`
//! point when those slices land.
//!
//! The MFi transport is build-mode-gated: debug builds connect through
//! `RemoteI2c` to the device-side `bridgething-mfi-proxy` (host iteration
//! reaches the chip remotely), while release builds open `/dev/i2c-3`
//! directly. Reading `SUPERBIRD_HOST` selects the device for the dev
//! path; production never consults it.

use std::collections::HashMap;

use bluer::rfcomm::{ConnectRequest, Profile, ProfileHandle, Role};
use bluer::{Address, Session};
use bridgething_iap2::csm::identification::{CarthingIdentification, IdentificationConfig};
use bridgething_iap2::csm::now_playing::{
  MediaItemAttributes, NowPlayingUpdate as Iap2NowPlayingUpdate, PlaybackAttributes, PlaybackState, RepeatMode,
};
use bridgething_iap2::session::WorkerMfiAccess;
use bridgething_iap2::{
  IAP2_ACCESSORY_UUID, IAP2_RFCOMM_CHANNEL, Iap2Command, Iap2Event, Iap2Session, Link, LinkConfig, Lsp, SessionEvent,
};
use bridgething_mfi::MfiAuth;
use futures::StreamExt;
use libbridgething::{MediaItemUpdate, NowPlayingUpdate, PlaybackUpdate};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::state::State;

use super::BluetoothResult;

const IAP2_PROFILE_NAME: &str = "iAP2";
const IAP2_CHANNEL_CAPACITY: usize = 16;

#[derive(Debug)]
struct ActiveSession {
  _link_handle: JoinHandle<bridgething_iap2::Result<()>>,
  _session_handle: JoinHandle<bridgething_iap2::Result<()>>,
  _events_handle: JoinHandle<()>,
}

#[derive(Debug)]
pub struct Iap2Manager {
  handle: ProfileHandle,
  identification: IdentificationConfig,
  mfi_worker: WorkerMfiAccess,
  state: State,
  sessions: HashMap<Address, ActiveSession>,
}

impl Iap2Manager {
  pub async fn init(session: &Session, state: &State) -> BluetoothResult<Option<Self>> {
    let mfi_worker = match probe_and_spawn_worker().await {
      Ok(w) => w,
      Err(reason) => {
        tracing::warn!(%reason, "MFi probe failed; iAP2 disabled");
        return Ok(None);
      }
    };

    let profile = Profile {
      uuid: IAP2_ACCESSORY_UUID,
      name: Some(IAP2_PROFILE_NAME.to_string()),
      role: Some(Role::Server),
      channel: Some(IAP2_RFCOMM_CHANNEL as u16),
      require_authentication: Some(false),
      require_authorization: Some(false),
      ..Default::default()
    };

    let handle = session.register_profile(profile).await?;
    tracing::info!(channel = IAP2_RFCOMM_CHANNEL, "registered iAP2 RFCOMM profile");

    let identification = build_identification(state);

    Ok(Some(Self {
      handle,
      identification,
      mfi_worker,
      state: state.clone(),
      sessions: HashMap::new(),
    }))
  }

  pub fn spawn(mut self) -> JoinHandle<()> {
    tokio::spawn(async move { self.recv().await })
  }

  async fn recv(&mut self) {
    tracing::info!("iAP2 manager listening for iPhone connections");
    while let Some(request) = self.handle.next().await {
      if let Err(err) = self.accept(request).await {
        tracing::error!(?err, "iAP2 accept failed");
      }
    }
    tracing::error!("iAP2 profile handle stream ended - this should not happen");
  }

  async fn accept(&mut self, request: ConnectRequest) -> BluetoothResult<()> {
    let address = request.device();
    tracing::info!(%address, "iAP2 connect request");

    let stream = request.accept()?;

    self.sessions.remove(&address);

    let (link_command_tx, link_command_rx) = mpsc::channel::<Iap2Command>(IAP2_CHANNEL_CAPACITY);
    let (link_events_tx, link_events_rx) = mpsc::channel::<Iap2Event>(IAP2_CHANNEL_CAPACITY);
    let (session_events_tx, session_events_rx) = mpsc::channel::<SessionEvent>(IAP2_CHANNEL_CAPACITY);

    let link_config = LinkConfig::new(Lsp::accessory_default());
    let _link_handle = tokio::spawn(Link::run(stream, link_config, link_events_tx, link_command_rx));

    let mfi = self.mfi_worker.handle();
    let session = Iap2Session::new(
      self.identification.clone(),
      mfi,
      link_command_tx,
      link_events_rx,
      session_events_tx,
    );
    let _session_handle = tokio::spawn(session.run());

    let _events_handle = tokio::spawn(observe_session_events(address, session_events_rx, self.state.clone()));

    self.sessions.insert(
      address,
      ActiveSession {
        _link_handle,
        _session_handle,
        _events_handle,
      },
    );

    Ok(())
  }
}

async fn observe_session_events(address: Address, mut rx: mpsc::Receiver<SessionEvent>, state: State) {
  while let Some(event) = rx.recv().await {
    match event {
      SessionEvent::LinkEstablished(lsp) => {
        tracing::info!(
          %address,
          peer_max_outgoing = lsp.max_outgoing,
          peer_max_len = lsp.max_len,
          "iAP2 link Established",
        );
      }
      SessionEvent::Authenticated => tracing::info!(%address, "iAP2 authenticated"),
      SessionEvent::Identified => tracing::info!(%address, "iAP2 identified"),
      SessionEvent::AuthFailed => tracing::warn!(%address, "iAP2 auth failed"),
      SessionEvent::IdentificationRejected { rejected_params } => {
        tracing::warn!(%address, ?rejected_params, "iAP2 identification rejected");
      }
      SessionEvent::NowPlayingUpdate(update) => {
        let lib_update = translate_now_playing(update);
        tracing::debug!(%address, ?lib_update, "iAP2 now-playing delta");
        if let Err(err) = state.player.apply_now_playing(lib_update).await {
          tracing::warn!(%address, ?err, "failed to apply iAP2 now-playing delta");
        }
      }
      SessionEvent::LinkDown(reason) => tracing::info!(%address, %reason, "iAP2 link down"),
    }
  }
}

/// Translate the iap2 crate's wire-decoded NowPlaying delta into the
/// canonical lib type. Two scope changes happen here:
///
/// - iAP2's u64 persistent identifier becomes a namespaced string key
///   (`iap2:track:{hex}`) so it cannot collide with identifiers minted
///   by other transports (Android companion, future plugins).
/// - iAP2's u8 artwork reference becomes a namespaced string key
///   (`iap2:artwork:{n}`) for the same reason; FileTransfer-side
///   slice 5 will resolve it to bytes through the same key.
fn translate_now_playing(update: Iap2NowPlayingUpdate) -> NowPlayingUpdate {
  NowPlayingUpdate {
    media_item: update.media_item.map(translate_media_item),
    playback: update.playback.map(translate_playback),
  }
}

fn translate_media_item(media: MediaItemAttributes) -> MediaItemUpdate {
  MediaItemUpdate {
    persistent_id: media.persistent_id.map(|id| format!("iap2:track:{id:016x}")),
    title: media.title,
    album: media.album,
    artist: media.artist,
    liked: media.liked,
    artwork_id: media.artwork_id.map(|id| format!("iap2:artwork:{id}")),
    duration_ms: None,
  }
}

fn translate_playback(play: PlaybackAttributes) -> PlaybackUpdate {
  PlaybackUpdate {
    playing: play.state.map(|s| matches!(s, PlaybackState::Playing)),
    position_ms: play.position_ms,
    shuffle: play.shuffle,
    repeat: play.repeat.map(RepeatMode::as_u32),
    app_bundle: play.app_bundle,
    app_display_name: play.app_display_name,
  }
}

fn build_identification(state: &State) -> IdentificationConfig {
  let bt_mac = parse_bt_mac(&state.meta.bt_mac);
  IdentificationConfig::for_carthing(CarthingIdentification {
    serial_number: state.meta.serial_number.clone(),
    firmware_version: format!("v{}", env!("CARGO_PKG_VERSION")),
    bt_mac,
  })
}

/// Parse a `XX:XX:XX:XX:XX:XX` BT MAC into the byte order iAP2's
/// transport-component group expects (big-endian, same order BlueZ
/// prints). Returns all-zeros and warns on malformed input - we'd
/// rather attempt identification with a sentinel MAC than refuse to
/// register iAP2 at all.
fn parse_bt_mac(s: &str) -> [u8; 6] {
  let parts: Vec<&str> = s.split(':').collect();
  if parts.len() != 6 {
    tracing::warn!(meta_bt_mac = %s, "unexpected bt_mac format; iAP2 transport component MAC will be all zeros");
    return [0; 6];
  }
  let mut out = [0u8; 6];
  for (i, part) in parts.iter().enumerate() {
    match u8::from_str_radix(part, 16) {
      Ok(b) => out[i] = b,
      Err(_) => {
        tracing::warn!(meta_bt_mac = %s, "non-hex byte in bt_mac; iAP2 transport component MAC will be all zeros");
        return [0; 6];
      }
    }
  }
  out
}

#[cfg(debug_assertions)]
async fn probe_and_spawn_worker() -> Result<WorkerMfiAccess, String> {
  use bridgething_mfi::RemoteI2c;

  let host = std::env::var("SUPERBIRD_HOST").map_err(|_| "SUPERBIRD_HOST env not set".to_string())?;
  let addr = format!("{host}:9090");

  let mfi = tokio::task::spawn_blocking(move || -> Result<MfiAuth<RemoteI2c>, String> {
    let transport = RemoteI2c::connect(addr.as_str()).map_err(|e| format!("RemoteI2c::connect({addr}): {e:?}"))?;
    let mut auth = MfiAuth::with_transport(transport);
    auth.cert().map_err(|e| format!("cert probe: {e:?}"))?;
    Ok(auth)
  })
  .await
  .map_err(|e| format!("MFi probe task panicked: {e:?}"))??;

  tracing::info!("MFi probe via RemoteI2c succeeded; spawning iap2-mfi-worker");
  Ok(WorkerMfiAccess::spawn(mfi))
}

#[cfg(not(debug_assertions))]
async fn probe_and_spawn_worker() -> Result<WorkerMfiAccess, String> {
  let mfi = tokio::task::spawn_blocking(|| -> Result<MfiAuth<bridgething_mfi::LinuxI2c>, String> {
    let mut auth = MfiAuth::open_default().map_err(|e| format!("MfiAuth::open_default: {e:?}"))?;
    auth.cert().map_err(|e| format!("cert probe: {e:?}"))?;
    Ok(auth)
  })
  .await
  .map_err(|e| format!("MFi probe task panicked: {e:?}"))??;

  tracing::info!("MFi probe via /dev/i2c-3 succeeded; spawning iap2-mfi-worker");
  Ok(WorkerMfiAccess::spawn(mfi))
}
