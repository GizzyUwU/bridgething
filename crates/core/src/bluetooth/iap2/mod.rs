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

use std::{collections::HashMap, time::Duration};

use bluer::{
  Adapter, Address, Session,
  rfcomm::{ConnectRequest, Profile, ProfileHandle, Role},
};
use bridgething_iap2::{
  HidCommand, IAP2_ACCESSORY_UUID, IAP2_DEVICE_UUID, IAP2_RFCOMM_CHANNEL, Iap2Command, Iap2Event, Iap2Session, Link,
  LinkConfig, Lsp, SessionEvent,
  csm::{
    identification::{CarthingIdentification, EaProtocol, EaProtocolMatchAction, IdentificationConfig},
    now_playing::{
      MediaItemAttributes, NowPlayingUpdate as Iap2NowPlayingUpdate, PlaybackAttributes, PlaybackState, RepeatMode,
    },
  },
  session::WorkerMfiAccess,
};
use bridgething_mfi::MfiAuth;
pub use ea::{Iap2EaGateway, Iap2EaGatewayHandle, StreamClosed, StreamOpened};
use futures::StreamExt;
use libbridgething::{DeviceType, MediaItemUpdate, NowPlayingUpdate, PeerIap2Status, PlaybackUpdate};
use tokio::{sync::mpsc, task::JoinHandle};

mod ea;

use super::{BluetoothResult, profiles::ProfileMan};
use crate::state::State;

const IAP2_PROFILE_NAME: &str = "iAP2";
const IAP2_CLIENT_PROFILE_NAME: &str = "iAP2 (device dial-in)";
const IAP2_CHANNEL_CAPACITY: usize = 16;
const COMPANION_BUNDLE_ID: &str = "com.bridgething.gateway";

const RECONNECT_INITIAL_DELAY: Duration = Duration::from_secs(2);
const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(60);
const RECONNECT_KICK_CAPACITY: usize = 16;

/// Custom SDP record advertised for the iAP2 RFCOMM listener. iOS does
/// not engage iAP2 on the auto-generated record bluez emits when only
/// `Profile { uuid, channel, ... }` is supplied - it specifically wants
/// the BluetoothProfileDescriptorList entry pointing at SerialPort
/// (0x1101 v1.0). Without that, iOS Bluetooth Settings shows
/// "<name> is Not Supported" without ever opening RFCOMM. The shape
/// here mirrors what the wiomoc-iap2 reference implementation publishes,
/// adjusted for our UUID and channel. Attribute ids:
///   0x0001 ServiceClassIDList            -> the iAP2 accessory UUID
///   0x0004 ProtocolDescriptorList        -> L2CAP + RFCOMM/<channel>
///   0x0005 BrowseGroupList               -> PublicBrowseGroup
///   0x0006 LanguageBaseAttributeIDList   -> en/UTF-8/0x0100
///   0x0008 ServiceAvailability           -> 0xff (fully available)
///   0x0009 BluetoothProfileDescriptorList -> SerialPort 0x1101 v1.0
///   0x0100 ServiceName (en)              -> "Wireless iAP"
fn iap2_service_record() -> String {
  format!(
    r#"<?xml version="1.0" encoding="UTF-8" ?>
<record>
    <attribute id="0x0001"><sequence><uuid value="{uuid}" /></sequence></attribute>
    <attribute id="0x0004"><sequence>
        <sequence><uuid value="0x0100" /></sequence>
        <sequence><uuid value="0x0003" /><uint8 value="0x{channel:02x}" /></sequence>
    </sequence></attribute>
    <attribute id="0x0005"><sequence><uuid value="0x1002" /></sequence></attribute>
    <attribute id="0x0006"><sequence>
        <uint16 value="0x656e" />
        <uint16 value="0x006a" />
        <uint16 value="0x0100" />
    </sequence></attribute>
    <attribute id="0x0008"><uint8 value="0xff" /></attribute>
    <attribute id="0x0009"><sequence>
        <sequence><uuid value="0x1101" /><uint16 value="0x0100" /></sequence>
    </sequence></attribute>
    <attribute id="0x0100"><text value="Wireless iAP" /></attribute>
</record>
"#,
    uuid = IAP2_ACCESSORY_UUID,
    channel = IAP2_RFCOMM_CHANNEL,
  )
}

#[derive(Debug)]
struct ActiveSession {
  hid_tx: mpsc::Sender<HidCommand>,
  _link_handle: JoinHandle<bridgething_iap2::Result<()>>,
  _session_handle: JoinHandle<bridgething_iap2::Result<()>>,
  _events_handle: JoinHandle<()>,
}

/// Cloneable handle to kick the iAP2 reconnect loop for a given peer.
/// Held by the daemon entry points that learn an iOS peer needs the
/// link brought up: startup, LinkDown observers, the stock-webapp
/// "connect to device" command. Sending is fire-and-forget; the
/// manager dedups outstanding tasks per mac.
#[derive(Debug, Clone)]
pub struct Iap2ReconnectHandle {
  tx: mpsc::Sender<Address>,
}

impl Iap2ReconnectHandle {
  pub async fn kick(&self, mac: Address) {
    if self.tx.send(mac).await.is_err() {
      tracing::debug!(%mac, "iap2 reconnect kick dropped; manager exited");
    }
  }
}

/// Cloneable handle the daemon's `TransportController` uses to dispatch
/// outbound HID commands when iAP2 (not the companion) holds playback
/// authority. Each send routes to the currently-active session's
/// HID flow; if no session is active the command is dropped with a
/// trace log.
#[derive(Debug, Clone)]
pub struct Iap2TransportHandle {
  tx: mpsc::Sender<HidCommand>,
}

impl Iap2TransportHandle {
  pub async fn send(&self, cmd: HidCommand) {
    if self.tx.send(cmd).await.is_err() {
      tracing::debug!(?cmd, "iap2 transport command dropped; manager exited");
    }
  }
}

#[derive(Debug)]
pub struct Iap2Manager {
  server_handle: ProfileHandle,
  client_handle: ProfileHandle,
  identification: IdentificationConfig,
  mfi_worker: WorkerMfiAccess,
  state: State,
  profile_man: ProfileMan,
  ea_gateway: Iap2EaGatewayHandle,
  adapter: Adapter,
  sessions: HashMap<Address, ActiveSession>,
  reconnects: HashMap<Address, JoinHandle<()>>,
  reconnect_tx: mpsc::Sender<Address>,
  reconnect_rx: mpsc::Receiver<Address>,
  transport_rx: mpsc::Receiver<HidCommand>,
}

impl Iap2Manager {
  pub async fn init(
    session: &Session,
    adapter: Adapter,
    state: &State,
    profile_man: ProfileMan,
    ea_gateway: Iap2EaGatewayHandle,
  ) -> BluetoothResult<Option<(Self, Iap2ReconnectHandle, Iap2TransportHandle)>> {
    let mfi_worker = match probe_and_spawn_worker().await {
      Ok(w) => w,
      Err(reason) => {
        tracing::warn!(%reason, "MFi probe failed; iAP2 disabled");
        return Ok(None);
      }
    };

    let server_profile = Profile {
      uuid: IAP2_ACCESSORY_UUID,
      name: Some(IAP2_PROFILE_NAME.to_string()),
      role: Some(Role::Server),
      channel: Some(IAP2_RFCOMM_CHANNEL as u16),
      require_authentication: Some(false),
      require_authorization: Some(false),
      auto_connect: Some(true),
      service_record: Some(iap2_service_record()),
      ..Default::default()
    };
    let server_handle = session.register_profile(server_profile).await?;
    tracing::info!(channel = IAP2_RFCOMM_CHANNEL, "registered iAP2 RFCOMM server profile");

    let client_profile = Profile {
      uuid: IAP2_DEVICE_UUID,
      name: Some(IAP2_CLIENT_PROFILE_NAME.to_string()),
      role: Some(Role::Client),
      require_authentication: Some(false),
      require_authorization: Some(false),
      auto_connect: Some(true),
      ..Default::default()
    };
    let client_handle = session.register_profile(client_profile).await?;
    tracing::info!("registered iAP2 RFCOMM client profile (accessory-initiated reconnect)");

    let identification = build_identification(state);

    let (reconnect_tx, reconnect_rx) = mpsc::channel(RECONNECT_KICK_CAPACITY);
    let reconnect_handle = Iap2ReconnectHandle {
      tx: reconnect_tx.clone(),
    };

    let (transport_tx, transport_rx) = mpsc::channel(IAP2_CHANNEL_CAPACITY);
    let transport_handle = Iap2TransportHandle { tx: transport_tx };

    Ok(Some((
      Self {
        server_handle,
        client_handle,
        identification,
        mfi_worker,
        state: state.clone(),
        profile_man,
        ea_gateway,
        adapter,
        sessions: HashMap::new(),
        reconnects: HashMap::new(),
        reconnect_tx,
        reconnect_rx,
        transport_rx,
      },
      reconnect_handle,
      transport_handle,
    )))
  }

  pub fn spawn(mut self) -> JoinHandle<()> {
    tokio::spawn(async move { self.recv().await })
  }

  async fn recv(&mut self) {
    tracing::info!("iAP2 manager listening for iPhone connections");
    self.kickoff_reconnects_for_paired_ios().await;

    loop {
      tokio::select! {
        Some(request) = self.server_handle.next() => {
          if let Err(err) = self.accept(request, ConnectDirection::Inbound).await {
            tracing::error!(?err, "iAP2 server accept failed");
          }
        }
        Some(request) = self.client_handle.next() => {
          if let Err(err) = self.accept(request, ConnectDirection::Outbound).await {
            tracing::error!(?err, "iAP2 client accept failed");
          }
        }
        Some(mac) = self.reconnect_rx.recv() => {
          self.spawn_reconnect(mac);
        }
        Some(cmd) = self.transport_rx.recv() => {
          self.dispatch_transport(cmd).await;
        }
        else => {
          tracing::error!("iAP2 manager streams all ended - this should not happen");
          return;
        }
      }
    }
  }

  async fn accept(&mut self, request: ConnectRequest, direction: ConnectDirection) -> BluetoothResult<()> {
    let address = request.device();
    tracing::info!(%address, ?direction, "iAP2 connect request");

    let stream = request.accept()?;

    self.sessions.remove(&address);
    self.cancel_reconnect(&address);

    let (link_command_tx, link_command_rx) = mpsc::channel::<Iap2Command>(IAP2_CHANNEL_CAPACITY);
    let (link_events_tx, link_events_rx) = mpsc::channel::<Iap2Event>(IAP2_CHANNEL_CAPACITY);
    let (session_events_tx, session_events_rx) = mpsc::channel::<SessionEvent>(IAP2_CHANNEL_CAPACITY);
    let (hid_tx, hid_rx) = mpsc::channel::<HidCommand>(IAP2_CHANNEL_CAPACITY);

    let link_config = LinkConfig::new(Lsp::accessory_default());
    let _link_handle = tokio::spawn(Link::run(stream, link_config, link_events_tx, link_command_rx));

    let mfi = self.mfi_worker.handle();
    let session = Iap2Session::with_app_launch(
      self.identification.clone(),
      Some(COMPANION_BUNDLE_ID.to_string()),
      mfi,
      link_command_tx,
      link_events_rx,
      session_events_tx,
      hid_rx,
    );
    let _session_handle = tokio::spawn(session.run());

    let _events_handle = tokio::spawn(observe_session_events(
      address,
      session_events_rx,
      self.state.clone(),
      self.profile_man.clone(),
      self.ea_gateway.clone(),
      self.reconnect_tx.clone(),
    ));

    self.sessions.insert(
      address,
      ActiveSession {
        hid_tx,
        _link_handle,
        _session_handle,
        _events_handle,
      },
    );

    Ok(())
  }

  /// Forward a transport command to whichever active iAP2 session is up.
  /// In practice the Car Thing only ever has one iPhone connected, so the
  /// "first session" we find is the right one. If no session is active the
  /// command is dropped at trace level - the controller's authority check
  /// already gated on iAP2 being a viable target, so a missing session
  /// here means a race with link teardown, not a bug.
  async fn dispatch_transport(&self, cmd: HidCommand) {
    let Some(session) = self.sessions.values().next() else {
      tracing::trace!(?cmd, "iAP2 transport command with no active session; dropping");
      return;
    };
    if session.hid_tx.send(cmd).await.is_err() {
      tracing::debug!(?cmd, "iAP2 session HID receiver closed; dropping command");
    }
  }

  async fn kickoff_reconnects_for_paired_ios(&mut self) {
    let addresses = match self.adapter.device_addresses().await {
      Ok(a) => a,
      Err(err) => {
        tracing::warn!(?err, "failed to enumerate paired devices for iAP2 reconnect");
        return;
      }
    };
    for mac in addresses {
      if self.peer_advertises_iap2(mac).await {
        self.spawn_reconnect(mac);
      }
    }
  }

  async fn peer_advertises_iap2(&self, mac: Address) -> bool {
    let Ok(device) = self.adapter.device(mac) else {
      return false;
    };
    if !device.is_paired().await.unwrap_or(false) {
      return false;
    }
    device
      .uuids()
      .await
      .ok()
      .flatten()
      .is_some_and(|set| set.contains(&IAP2_DEVICE_UUID))
  }

  fn spawn_reconnect(&mut self, mac: Address) {
    if self.sessions.contains_key(&mac) {
      tracing::trace!(%mac, "iAP2 session already active; skipping reconnect kick");
      return;
    }
    if let Some(handle) = self.reconnects.get(&mac)
      && !handle.is_finished() {
        tracing::trace!(%mac, "iAP2 reconnect already in flight; skipping kick");
        return;
      }
    let adapter = self.adapter.clone();
    let state = self.state.clone();
    let task = tokio::spawn(reconnect_loop(adapter, state, mac));
    self.reconnects.insert(mac, task);
  }

  fn cancel_reconnect(&mut self, mac: &Address) {
    if let Some(handle) = self.reconnects.remove(mac) {
      handle.abort();
    }
  }
}

#[derive(Debug, Clone, Copy)]
enum ConnectDirection {
  Inbound,
  Outbound,
}

async fn reconnect_loop(adapter: Adapter, state: State, mac: Address) {
  let device = match adapter.device(mac) {
    Ok(d) => d,
    Err(err) => {
      tracing::debug!(%mac, ?err, "iAP2 reconnect aborted: no device handle");
      return;
    }
  };
  let mut delay = RECONNECT_INITIAL_DELAY;
  loop {
    tokio::time::sleep(delay).await;

    if !still_should_reconnect(&adapter, &state, mac).await {
      tracing::debug!(%mac, "iAP2 reconnect loop exiting (peer unbonded or already up)");
      return;
    }

    tracing::debug!(%mac, ?delay, "dialing iPhone iAP2-device channel");
    match device.connect_profile(&IAP2_DEVICE_UUID).await {
      Ok(()) => {
        tracing::info!(%mac, "iAP2 connect_profile dial succeeded; awaiting NewConnection");
        return;
      }
      Err(err) => {
        tracing::debug!(%mac, ?err, "iAP2 connect_profile dial failed; backing off");
        delay = (delay * 2).min(RECONNECT_MAX_DELAY);
      }
    }
  }
}

async fn still_should_reconnect(adapter: &Adapter, state: &State, mac: Address) -> bool {
  let Ok(device) = adapter.device(mac) else {
    return false;
  };
  if !device.is_paired().await.unwrap_or(false) {
    return false;
  }
  if device.is_connected().await.unwrap_or(false)
    && let Some(peer) = state.peers.get(&mac).await
      && peer.iap2 != PeerIap2Status::None {
        return false;
      }
  true
}

async fn observe_session_events(
  address: Address,
  mut rx: mpsc::Receiver<SessionEvent>,
  state: State,
  profile_man: ProfileMan,
  ea_gateway: Iap2EaGatewayHandle,
  reconnect_tx: mpsc::Sender<Address>,
) {
  // Tracks the most recent `persistent_id` hex form we've seen across
  // NowPlayingUpdate deltas. iAP2 sends MediaItemAttributes only when a
  // field actually changed, so a delta carrying just `artwork_id`
  // doesn't repeat persistent_id; we have to remember it. Used both to
  // synthesise the asset-id URL on outbound NowPlayingUpdates and to
  // tag inbound FileTransfer bytes with their owning track.
  let mut last_persistent_hex: Option<String> = None;

  while let Some(event) = rx.recv().await {
    match event {
      SessionEvent::LinkEstablished(lsp) => {
        tracing::info!(
          %address,
          peer_max_outgoing = lsp.max_outgoing,
          peer_max_len = lsp.max_len,
          "iAP2 link Established",
        );
        if let Err(err) = profile_man.upsert_paired_device(address, DeviceType::Ios).await {
          tracing::warn!(%address, ?err, "failed to upsert peer for iAP2 link");
        }
        let _ = state.peers.set_iap2(address, PeerIap2Status::LinkUp).await;
      }
      SessionEvent::Authenticated => {
        tracing::info!(%address, "iAP2 authenticated");
        let _ = state.peers.set_iap2(address, PeerIap2Status::Authenticated).await;
      }
      SessionEvent::Identified => {
        tracing::info!(%address, "iAP2 identified");
        let _ = state.peers.set_iap2(address, PeerIap2Status::Identified).await;
      }
      SessionEvent::AuthFailed => tracing::warn!(%address, "iAP2 auth failed"),
      SessionEvent::IdentificationRejected { rejected_params } => {
        tracing::warn!(%address, ?rejected_params, "iAP2 identification rejected");
      }
      SessionEvent::NowPlayingUpdate(update) => {
        if let Some(pid) = update.media_item.as_ref().and_then(|m| m.persistent_id) {
          last_persistent_hex = Some(format!("{pid:016x}"));
        }
        let lib_update = translate_now_playing(update, last_persistent_hex.as_deref());
        tracing::debug!(%address, ?lib_update, "iAP2 now-playing delta");
        if let Err(err) = state
          .player
          .apply_now_playing(crate::player::NowPlayingSource::Iap2, lib_update)
          .await
        {
          tracing::warn!(%address, ?err, "failed to apply iAP2 now-playing delta");
        }
      }
      SessionEvent::ArtworkBytes { transfer_id, bytes } => {
        let Some(pid_hex) = last_persistent_hex.as_deref() else {
          tracing::warn!(%address, transfer_id, "iAP2 artwork bytes received before any NowPlayingUpdate; dropping");
          continue;
        };
        let id = format!("iap2/art/{pid_hex}/{transfer_id}");
        tracing::debug!(%address, asset_id = %id, bytes = bytes.len(), "iAP2 artwork bytes -> AssetCache");
        if let Err(err) = state
          .assets
          .insert(
            id,
            tokio_util::bytes::Bytes::copy_from_slice(&bytes),
            Some("image/jpeg".to_string()),
            libbridgething::AssetRetention::Lru,
          )
          .await
        {
          tracing::warn!(%address, ?err, "failed to insert iAP2 artwork into asset cache");
        }
      }
      SessionEvent::EaStreamOpened {
        stream_id,
        protocol_id,
        inbound_rx,
        outbound,
      } => {
        tracing::info!(%address, stream_id, protocol_id, "iAP2 EA stream opened");
        ea_gateway
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
        ea_gateway.notify_closed(StreamClosed { address, stream_id }).await;
      }
      SessionEvent::LinkDown(reason) => {
        tracing::info!(%address, %reason, "iAP2 link down");
        let _ = state.peers.set_iap2(address, PeerIap2Status::None).await;
        if reconnect_tx.send(address).await.is_err() {
          tracing::debug!(%address, "iap2 reconnect channel closed; not requeuing");
        }
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
    artist: media.artist,
    liked: media.liked,
    artwork_id,
    duration_ms: None,
  }
}

fn translate_playback(play: PlaybackAttributes) -> PlaybackUpdate {
  PlaybackUpdate {
    playing: play.state.map(|s| matches!(s, PlaybackState::Playing)),
    position_ms: play.position_ms,
    shuffle: play.shuffle,
    repeat: play.repeat.map(translate_repeat),
    app_bundle: play.app_bundle,
    app_display_name: play.app_display_name,
  }
}

fn translate_repeat(mode: RepeatMode) -> libbridgething::RepeatMode {
  match mode {
    RepeatMode::Off => libbridgething::RepeatMode::Off,
    RepeatMode::Track => libbridgething::RepeatMode::One,
    RepeatMode::All => libbridgething::RepeatMode::All,
  }
}

fn build_identification(state: &State) -> IdentificationConfig {
  let bt_mac = parse_bt_mac(&state.meta.bt_mac);
  let mut config = IdentificationConfig::for_carthing(CarthingIdentification {
    serial_number: state.meta.serial_number.clone(),
    firmware_version: format!("v{}", env!("CARGO_PKG_VERSION")),
    bt_mac,
  });
  config.supported_external_accessory_protocols = vec![EaProtocol {
    id: 1,
    name: COMPANION_BUNDLE_ID.to_string(),
    match_action: EaProtocolMatchAction::NoAlertAction,
    native_transport_component_identifier: None,
  }];
  config
}

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
