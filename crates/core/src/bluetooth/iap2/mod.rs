//! iAP2 over RFCOMM. Registers the iAP2 accessory profile, accepts
//! iPhone connect requests, and spawns one [`Iap2Session`] per active
//! link. MFi chip access is required;
//! initialization probes the chip first and skips profile registration
//! entirely if the probe fails (Car Things without working MFi silicon
//! still get a usable daemon, just no iOS support).
//!
//! Session events are emitted upstream via the public `Iap2Event`
//! channel returned from `init`. The daemon's main loop reads from
//! that channel and routes each event through `Iap2EventRouter`. The
//! manager itself stays out of state mutation.
//!
//! The MFi transport is build-mode-gated: debug builds connect through
//! `RemoteI2c` to the device-side `bridgething-mfi-proxy` (host iteration
//! reaches the chip remotely), while release builds open `/dev/i2c-3`
//! directly. Reading `SUPERBIRD_HOST` selects the device for the dev
//! path; production never consults it.

use std::{
  collections::{HashMap, HashSet},
  sync::Arc,
  time::Duration,
};

use bluer::{
  Adapter, Address, Session,
  rfcomm::{ConnectRequest, Profile, ProfileHandle, Role},
};
use bridgething_iap2::{
  HidCommand, IAP2_ACCESSORY_UUID, IAP2_DEVICE_UUID, IAP2_RFCOMM_CHANNEL, Iap2Command, Iap2Event as Iap2InternalEvent,
  Iap2Session, Link, LinkConfig, Lsp, NowPlayingCommand, SessionEvent,
  csm::identification::{CarthingIdentification, EaProtocol, EaProtocolMatchAction, IdentificationConfig},
  session::{TelephonyCommand, WorkerMfiAccess},
};
use bridgething_mfi::MfiAuth;
pub use ea::{Iap2EaGateway, Iap2EaGatewayHandle, StreamClosed, StreamOpened};
use futures::StreamExt;
use tokio::{
  sync::{RwLock, mpsc},
  task::JoinHandle,
};

mod ea;

use super::BluetoothResult;
use crate::state::meta::SuperbirdMeta;

const IAP2_PROFILE_NAME: &str = "iAP2";
const IAP2_CLIENT_PROFILE_NAME: &str = "iAP2 (device dial-in)";
const IAP2_CHANNEL_CAPACITY: usize = 16;
const IAP2_EVENTS_CAPACITY: usize = 64;
const COMPANION_BUNDLE_ID: &str = "com.bridgething.gateway";

const RECONNECT_INITIAL_DELAY: Duration = Duration::from_secs(2);
const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(60);
const RECONNECT_KICK_CAPACITY: usize = 16;
const SESSION_DEAD_CAPACITY: usize = 16;

/// SDP record advertised for the iAP2 RFCOMM listener. iOS does
/// not engage iAP2 on the auto-generated record bluez emits when only
/// `Profile { uuid, channel, ... }` is supplied - it specifically wants
/// the BluetoothProfileDescriptorList entry pointing at SerialPort
/// (0x1101 v1.0). Without that, iOS Bluetooth Settings shows
/// "<name> is Not Supported" without ever opening RFCOMM.
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
pub struct Iap2Event {
  pub address: Address,
  pub event: SessionEvent,
}

pub type Iap2EventsRx = mpsc::Receiver<Iap2Event>;

#[cfg(feature = "test-tap")]
pub type Iap2InjectTx = mpsc::Sender<Iap2Event>;

#[derive(Debug)]
struct ActiveSession {
  generation: u64,
  hid_tx: mpsc::Sender<HidCommand>,
  np_tx: mpsc::Sender<NowPlayingCommand>,
  tel_tx: mpsc::Sender<TelephonyCommand>,
  _link_handle: JoinHandle<bridgething_iap2::Result<()>>,
  _session_handle: JoinHandle<bridgething_iap2::Result<()>>,
  _shovel_handle: JoinHandle<()>,
}

#[derive(Debug, Clone, Copy)]
pub enum Iap2TransportCommand {
  Hid(HidCommand),
  NowPlaying(NowPlayingCommand),
}

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

#[derive(Debug, Clone)]
pub struct Iap2TransportHandle {
  tx: mpsc::Sender<Iap2TransportCommand>,
}

impl Iap2TransportHandle {
  pub async fn send_hid(&self, cmd: HidCommand) {
    if self.tx.send(Iap2TransportCommand::Hid(cmd)).await.is_err() {
      tracing::debug!(?cmd, "iap2 transport command dropped; manager exited");
    }
  }

  pub async fn send_now_playing(&self, cmd: NowPlayingCommand) {
    if self.tx.send(Iap2TransportCommand::NowPlaying(cmd)).await.is_err() {
      tracing::debug!(?cmd, "iap2 transport command dropped; manager exited");
    }
  }
}

#[derive(Debug, Clone)]
pub struct Iap2TelephonyHandle {
  tx: mpsc::Sender<TelephonyCommand>,
}

impl Iap2TelephonyHandle {
  pub async fn send(&self, cmd: TelephonyCommand) {
    if self.tx.send(cmd).await.is_err() {
      tracing::debug!("iap2 telephony command dropped; manager exited");
    }
  }
}

#[derive(Debug, Clone, Default)]
pub struct Iap2ActiveSessions {
  inner: Arc<RwLock<HashSet<Address>>>,
}

impl Iap2ActiveSessions {
  pub async fn insert(&self, mac: Address) {
    self.inner.write().await.insert(mac);
  }

  pub async fn remove(&self, mac: &Address) {
    self.inner.write().await.remove(mac);
  }

  pub async fn contains(&self, mac: &Address) -> bool {
    self.inner.read().await.contains(mac)
  }
}

#[derive(Debug)]
pub struct Iap2Manager {
  server_handle: ProfileHandle,
  client_handle: ProfileHandle,
  identification: IdentificationConfig,
  mfi_worker: WorkerMfiAccess,
  adapter: Adapter,
  sessions: HashMap<Address, ActiveSession>,
  active_sessions: Iap2ActiveSessions,
  reconnects: HashMap<Address, JoinHandle<()>>,
  reconnect_rx: mpsc::Receiver<Address>,
  transport_rx: mpsc::Receiver<Iap2TransportCommand>,
  telephony_rx: mpsc::Receiver<TelephonyCommand>,
  events_tx: mpsc::Sender<Iap2Event>,
  next_generation: u64,
  session_dead_tx: mpsc::Sender<(Address, u64)>,
  session_dead_rx: mpsc::Receiver<(Address, u64)>,
}

#[derive(Debug, Clone)]
pub struct Iap2Handles {
  pub reconnect: Iap2ReconnectHandle,
  pub transport: Iap2TransportHandle,
  pub telephony: Iap2TelephonyHandle,
}

pub(super) struct Iap2Bootstrap {
  reconnect_rx: mpsc::Receiver<Address>,
  transport_rx: mpsc::Receiver<Iap2TransportCommand>,
  telephony_rx: mpsc::Receiver<TelephonyCommand>,
  events_tx: mpsc::Sender<Iap2Event>,
  session_dead_tx: mpsc::Sender<(Address, u64)>,
  session_dead_rx: mpsc::Receiver<(Address, u64)>,
}

#[cfg(feature = "test-tap")]
impl Iap2Bootstrap {
  pub(super) fn events_tx(&self) -> Iap2InjectTx {
    self.events_tx.clone()
  }
}

pub(super) fn allocate_iap2() -> (Iap2Handles, Iap2EventsRx, Iap2Bootstrap) {
  let (reconnect_tx, reconnect_rx) = mpsc::channel(RECONNECT_KICK_CAPACITY);
  let (transport_tx, transport_rx) = mpsc::channel::<Iap2TransportCommand>(IAP2_CHANNEL_CAPACITY);
  let (telephony_tx, telephony_rx) = mpsc::channel::<TelephonyCommand>(IAP2_CHANNEL_CAPACITY);
  let (events_tx, events_rx) = mpsc::channel::<Iap2Event>(IAP2_EVENTS_CAPACITY);
  let (session_dead_tx, session_dead_rx) = mpsc::channel::<(Address, u64)>(SESSION_DEAD_CAPACITY);

  let handles = Iap2Handles {
    reconnect: Iap2ReconnectHandle { tx: reconnect_tx },
    transport: Iap2TransportHandle { tx: transport_tx },
    telephony: Iap2TelephonyHandle { tx: telephony_tx },
  };
  let bootstrap = Iap2Bootstrap {
    reconnect_rx,
    transport_rx,
    telephony_rx,
    events_tx,
    session_dead_tx,
    session_dead_rx,
  };
  (handles, events_rx, bootstrap)
}

impl Iap2Manager {
  pub(super) async fn start(
    bootstrap: Iap2Bootstrap,
    session: &Session,
    adapter: Adapter,
    meta: &SuperbirdMeta,
  ) -> BluetoothResult<Option<JoinHandle<()>>> {
    let mfi_worker = match probe_and_spawn_worker().await {
      Ok(w) => w,
      Err(reason) => {
        tracing::warn!(%reason, "MFi probe failed; iAP2 disabled");
        drop(bootstrap);
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

    let identification = build_identification(meta);
    let Iap2Bootstrap {
      reconnect_rx,
      transport_rx,
      telephony_rx,
      events_tx,
      session_dead_rx,
      session_dead_tx,
    } = bootstrap;
    let active_sessions = Iap2ActiveSessions::default();

    let manager = Self {
      server_handle,
      client_handle,
      identification,
      mfi_worker,
      adapter,
      sessions: HashMap::new(),
      active_sessions,
      reconnects: HashMap::new(),
      reconnect_rx,
      transport_rx,
      telephony_rx,
      events_tx,
      next_generation: 0,
      session_dead_tx,
      session_dead_rx,
    };

    Ok(Some(manager.spawn()))
  }

  fn spawn(mut self) -> JoinHandle<()> {
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
          self.spawn_reconnect(mac).await;
        }
        Some((mac, generation)) = self.session_dead_rx.recv() => {
          self.handle_session_dead(mac, generation).await;
        }
        Some(cmd) = self.transport_rx.recv() => {
          self.dispatch_transport(cmd).await;
        }
        Some(cmd) = self.telephony_rx.recv() => {
          self.dispatch_telephony(cmd).await;
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
    self.active_sessions.remove(&address).await;
    self.cancel_reconnect(&address);

    let generation = self.next_generation;
    self.next_generation = self.next_generation.wrapping_add(1);

    let (link_command_tx, link_command_rx) = mpsc::channel::<Iap2Command>(IAP2_CHANNEL_CAPACITY);
    let (link_events_tx, link_events_rx) = mpsc::channel::<Iap2InternalEvent>(IAP2_CHANNEL_CAPACITY);
    let (session_events_tx, session_events_rx) = mpsc::channel::<SessionEvent>(IAP2_CHANNEL_CAPACITY);
    let (hid_tx, hid_rx) = mpsc::channel::<HidCommand>(IAP2_CHANNEL_CAPACITY);
    let (np_tx, np_rx) = mpsc::channel::<NowPlayingCommand>(IAP2_CHANNEL_CAPACITY);
    let (tel_tx, tel_rx) = mpsc::channel::<TelephonyCommand>(IAP2_CHANNEL_CAPACITY);

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
      np_rx,
      tel_rx,
    );
    let _session_handle = tokio::spawn(session.run());

    let _shovel_handle = tokio::spawn(shovel_session_events(
      address,
      generation,
      session_events_rx,
      self.events_tx.clone(),
      self.session_dead_tx.clone(),
    ));

    self.active_sessions.insert(address).await;
    self.sessions.insert(
      address,
      ActiveSession {
        generation,
        hid_tx,
        np_tx,
        tel_tx,
        _link_handle,
        _session_handle,
        _shovel_handle,
      },
    );

    Ok(())
  }

  async fn dispatch_transport(&self, cmd: Iap2TransportCommand) {
    let Some(session) = self.sessions.values().next() else {
      tracing::trace!(?cmd, "iAP2 transport command with no active session; dropping");
      return;
    };
    match cmd {
      Iap2TransportCommand::Hid(hid) => {
        if session.hid_tx.send(hid).await.is_err() {
          tracing::debug!(?hid, "iAP2 session HID receiver closed; dropping command");
        }
      }
      Iap2TransportCommand::NowPlaying(np) => {
        if session.np_tx.send(np).await.is_err() {
          tracing::debug!(?np, "iAP2 session NowPlaying receiver closed; dropping command");
        }
      }
    }
  }

  async fn dispatch_telephony(&self, cmd: TelephonyCommand) {
    let Some(session) = self.sessions.values().next() else {
      tracing::trace!(?cmd, "iAP2 telephony command with no active session; dropping");
      return;
    };
    if session.tel_tx.send(cmd).await.is_err() {
      tracing::debug!("iAP2 session telephony receiver closed; dropping command");
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
      self.spawn_reconnect(mac).await;
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

  async fn spawn_reconnect(&mut self, mac: Address) {
    if !self.peer_advertises_iap2(mac).await {
      tracing::trace!(%mac, "iAP2 reconnect skipped: peer does not advertise iAP2");
      return;
    }
    if self.sessions.contains_key(&mac) {
      tracing::trace!(%mac, "iAP2 session already active; skipping reconnect kick");
      return;
    }
    if let Some(handle) = self.reconnects.get(&mac)
      && !handle.is_finished()
    {
      tracing::trace!(%mac, "iAP2 reconnect already in flight; skipping kick");
      return;
    }
    let adapter = self.adapter.clone();
    let active = self.active_sessions.clone();
    let task = tokio::spawn(reconnect_loop(adapter, active, mac));
    self.reconnects.insert(mac, task);
  }

  fn cancel_reconnect(&mut self, mac: &Address) {
    if let Some(handle) = self.reconnects.remove(mac) {
      handle.abort();
    }
  }

  async fn handle_session_dead(&mut self, mac: Address, generation: u64) {
    match self.sessions.get(&mac) {
      Some(active) if active.generation == generation => {}
      Some(_) => {
        tracing::trace!(%mac, generation, "ignoring stale iAP2 session-dead signal");
        return;
      }
      None => {
        tracing::trace!(%mac, generation, "ignoring iAP2 session-dead signal: no active session");
        return;
      }
    }

    tracing::info!(%mac, "iAP2 session ended; cleaning up and re-arming reconnect");
    self.sessions.remove(&mac);
    self.active_sessions.remove(&mac).await;
    self.spawn_reconnect(mac).await;
  }
}

#[derive(Debug, Clone, Copy)]
enum ConnectDirection {
  Inbound,
  Outbound,
}

async fn reconnect_loop(adapter: Adapter, active_sessions: Iap2ActiveSessions, mac: Address) {
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

    if !still_should_reconnect(&adapter, &active_sessions, mac).await {
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

async fn still_should_reconnect(adapter: &Adapter, active_sessions: &Iap2ActiveSessions, mac: Address) -> bool {
  let Ok(device) = adapter.device(mac) else {
    return false;
  };
  if !device.is_paired().await.unwrap_or(false) {
    return false;
  }
  if device.is_connected().await.unwrap_or(false) && active_sessions.contains(&mac).await {
    return false;
  }
  true
}

async fn shovel_session_events(
  address: Address,
  generation: u64,
  mut rx: mpsc::Receiver<SessionEvent>,
  tx: mpsc::Sender<Iap2Event>,
  dead_tx: mpsc::Sender<(Address, u64)>,
) {
  while let Some(event) = rx.recv().await {
    if tx.send(Iap2Event { address, event }).await.is_err() {
      tracing::debug!(%address, "iap2 events channel closed; dropping shovel");
      break;
    }
  }
  if dead_tx.send((address, generation)).await.is_err() {
    tracing::trace!(%address, generation, "iap2 session-dead channel closed; manager exited");
  }
}

fn build_identification(meta: &SuperbirdMeta) -> IdentificationConfig {
  let bt_mac = parse_bt_mac(&meta.bt_mac);
  let mut config = IdentificationConfig::for_carthing(CarthingIdentification {
    serial_number: meta.serial_number.clone(),
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
