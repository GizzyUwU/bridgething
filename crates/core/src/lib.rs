mod bluetooth;
mod net;

mod als;
mod mic;
mod systemd;

mod asset;
mod authority;
mod capabilities;
mod chrome;
mod db;

mod handler;
mod input;
mod install;
mod ota;
mod paths;
mod peer;
mod player;
mod proxy;
mod state;
mod transfer;
mod transport;

mod stock;

mod monitoring;

use std::{future::Future, net::SocketAddr, path::PathBuf, pin::Pin};

use als::{AlsConfig, AlsManager};
use asset::{AssetCache, AssetIngest};
use authority::AuthorityRegistry;
use bluetooth::{BluetoothBringup, BluetoothDeps, BluetoothManager};
#[cfg(feature = "test-tap")]
pub use bluetooth::{Iap2Event, Iap2InjectTx};
use capabilities::CapabilitiesRegistry;
#[cfg(feature = "test-tap")]
pub use handler::client::{ClientMode, PossibleSendMsg};
use handler::{ClientHandler, GatewayHandler};
use libbridgething::{BRIDGETHING_NETWORK_GATEWAY_PORT, BRIDGETHING_STOCK_WS_PORT, BRIDGETHING_WS_MODERN_PORT};
use mic::{MicConfig, MicManager};
#[cfg(feature = "test-tap")]
pub use net::TappedFrame;
use ota::{OtaOrchestrator, OtaTerminators, RangeProxy};
use peer::PeerTracker;
use player::Player;
// don't pub anything else from core so that dead code lints still work
pub use state::{AppState, State};
use state::{
  AudioManager, DeviceStore, KvStore, MetaStore, RouteTable, StateAssembly, TelephonyManager, TimeManager,
  WebappRegistry,
};
use systemd::Notify;
use transfer::ChunkedTransfer;
use transport::TransportController;
pub struct Daemon {
  pub state: State,
  #[cfg(feature = "test-tap")]
  pub inject: Option<HeadlessInject>,
  #[cfg(feature = "test-tap")]
  pub server_addrs: ServerAddrs,
  loop_fut: Pin<Box<dyn Future<Output = ()> + Send>>,
}

#[cfg(feature = "test-tap")]
#[derive(Clone, Copy, Debug)]
pub struct ServerAddrs {
  pub stock: SocketAddr,
  pub modern: SocketAddr,
}

impl Daemon {
  pub async fn run(self) {
    self.loop_fut.await
  }
}

/// actual daemon entry point
pub async fn run_daemon() {
  init(DaemonConfig::real()).await.run().await;
}

pub async fn init(config: DaemonConfig) -> Daemon {
  let (log_tap, log_tap_layer) = state::LogTap::new();
  if config.install_logger {
    monitoring::init_logger(log_tap_layer);
  }

  let notifier = systemd::init_notifier();

  notifier.status("initializing bridgething...");
  let static_meta = state::meta::SuperbirdMeta::read_or_default().await;
  tracing::debug!("metadata: {:?}", &static_meta);

  let (client_man, mut client_listener) = net::create_client_manager();
  let bus = net::WireEventBus::new(client_man.clone());

  let (db, assets_blobs_dir, transfers_dir) = match config.state_dir.as_deref() {
    Some(dir) => {
      let assets = dir.join("assets");
      let transfers = dir.join("transfers");
      tokio::fs::create_dir_all(&assets)
        .await
        .expect("failed to create harness asset dir");
      tokio::fs::create_dir_all(&transfers)
        .await
        .expect("failed to create harness transfer dir");
      let db = db::open(None).await.expect("failed to open in-memory state database");
      (db, assets, transfers)
    }
    None => {
      let db = state::open_state_db().await.expect("failed to open state database");
      (db, paths::assets_blobs_dir(), paths::transfers_dir())
    }
  };

  let devices = DeviceStore::new(db.clone());
  let kv = KvStore::new(db.clone());
  let meta_store = MetaStore::new(db.clone());

  let meta = state::meta::DeviceMeta::init(static_meta, kv.clone()).await;

  let installed_webapps_root = config.webapps_dir.clone().unwrap_or_else(paths::webapps_dir);
  let builtin_webapps_root = config.ro_webapps_dir.clone().unwrap_or_else(paths::ro_webapps_dir);
  let webapps = WebappRegistry::init(installed_webapps_root, builtin_webapps_root)
    .await
    .expect("failed to initialize webapp registry");
  meta_store
    .enforce_active_webapp_exists(&webapps)
    .await
    .expect("failed to enforce active webapp invariant");

  let asset_pending = AssetCache::init(db.clone(), assets_blobs_dir)
    .await
    .expect("failed to initialize asset cache");
  let (assets, asset_cache_handle) = asset_pending.spawn();

  let transfer_pending = ChunkedTransfer::init(transfers_dir)
    .await
    .expect("failed to initialize chunked transfer manager");
  let (transfers, transfer_handle) = transfer_pending.spawn();

  let (ingest, ingest_handle) = AssetIngest::spawn(transfers.clone(), assets.clone());

  let ws_routes = RouteTable::new();
  let stream_routes = RouteTable::new();
  let geo_watchers = state::GeoWatchers::new();
  let tunnel_routes = state::TunnelRoutes::new();

  let authority = AuthorityRegistry::new();
  let capabilities = CapabilitiesRegistry::new(bus.clone(), authority.clone());
  let player = Player::new(bus.clone(), authority.clone());
  let audio = AudioManager::new(authority.clone(), bus.clone());
  let asset_wait = asset::wait::AssetWaitTracker::new();
  let _asset_invalidator = asset::wait::spawn_invalidator(assets.clone(), asset_wait.clone());
  let iap2_pending_art = handler::iap2::Iap2PendingArt::new();
  let peers = PeerTracker::new(
    bus.clone(),
    player.clone(),
    audio.clone(),
    capabilities.clone(),
    ws_routes.clone(),
    stream_routes.clone(),
  );

  let chrome = chrome::Chrome::init().await.expect("failed to initialize chrome");

  let (bluetooth_tx, mut bluetooth_rx) = tokio::sync::mpsc::channel(16);
  let bluetooth_deps = BluetoothDeps {
    bus: bus.clone(),
    meta: meta.clone(),
    devices: devices.clone(),
    peers: peers.clone(),
  };
  let (bluetooth, bluetooth_bootstrap) = BluetoothManager::create();

  let telephony = TelephonyManager::new(bus.clone(), bluetooth.iap2.telephony.clone());
  let time = TimeManager::new(bus.clone());

  let (als, als_handle) = AlsManager::init(bus.clone(), AlsConfig::default())
    .await
    .expect("failed to initialize ALS manager")
    .spawn();
  let (mic, mic_handle) = MicManager::init(bus.clone(), bluetooth.clone(), MicConfig::default())
    .await
    .spawn();

  let range_proxy_handle = RangeProxy::spawn(bluetooth.clone(), libbridgething::BRIDGETHING_OTA_RANGE_PROXY_PORT).await;

  let (ota_events_tx, ota_events_rx) = tokio::sync::mpsc::channel(64);
  let (ota, _ota_handle) = OtaOrchestrator::spawn(
    transfers.clone(),
    ota_events_tx,
    OtaTerminators {
      reboot: std::sync::Arc::new(trigger_reboot),
      restart_self: std::sync::Arc::new(trigger_restart_self),
    },
    range_proxy_handle.proxy.clone(),
  );

  let state = AppState::assemble(StateAssembly {
    client_man: client_man.clone(),
    bus,
    meta,
    player,
    chrome,
    webapps,
    assets,
    transfers: transfers.clone(),
    ingest,
    asset_wait,
    iap2_pending_art,
    authority,
    capabilities,
    peers,
    telephony,
    time,
    audio,
    als,
    mic,
    devices,
    kv,
    ws_routes,
    stream_routes,
    geo_watchers,
    log_tap,
    tunnel_routes,
    db,
    meta_store,
    asset_cache_handle,
    transfer_handle,
    ingest_handle,
    als_handle,
    mic_handle,
  });

  spawn_ota_event_forwarder(bluetooth.clone(), state.client_man.clone(), ota_events_rx);
  spawn_nickname_observer(state.meta.subscribe(), bluetooth.clone(), state.bus.clone());

  let transport = TransportController::new(
    state.authority.clone(),
    state.player.clone(),
    bluetooth.clone(),
    bluetooth.iap2.transport.clone(),
  );

  notifier.status("initializing server binds...");
  let server = net::Server::bind(state.clone(), config.stock_bind, config.modern_bind)
    .await
    .expect("failed to bind client servers");
  #[cfg(feature = "test-tap")]
  let server_addrs = ServerAddrs {
    stock: server.stock_addr(),
    modern: server.modern_addr(),
  };

  let client_handler = ClientHandler::new(state.clone(), bluetooth.clone(), transport);
  let gateway_handler = GatewayHandler::new(state.clone(), bluetooth.clone(), ota);

  let _input = input::InputManager::spawn(state.clone());

  if let Err(err) = proxy::spawn(state.clone(), bluetooth.clone()).await {
    tracing::warn!(
      ?err,
      "SOCKS proxy failed to bind; chromium net.proxy webapps will not work"
    );
  }

  notifier.status("initializing bluetooth stack...");
  #[cfg(feature = "test-tap")]
  let mut headless_inject = None;
  let bringup = match config.bluetooth {
    BluetoothMode::Real => BluetoothBringup::Real,
    #[cfg(feature = "test-tap")]
    BluetoothMode::Headless => {
      let (inject_tx, inject_rx) = bluetooth::inject_channel();
      headless_inject = Some(HeadlessInject {
        rfcomm: inject_tx,
        iap2: bluetooth_bootstrap.iap2_inject_tx(),
      });
      BluetoothBringup::Headless(inject_rx)
    }
  };
  let bluetooth_handle = bluetooth.spawn(
    bluetooth_bootstrap,
    bluetooth_deps,
    state.clone(),
    bluetooth_tx.clone(),
    config.network_bind,
    bringup,
  );

  notifier.ready(true, Some("ready to accept connections..."));

  let state_out = state.clone();
  let handle_signals = config.handle_signals;

  let loop_fut = Box::pin(async move {
    // keep alive in future
    let _asset_invalidator = _asset_invalidator;
    let _ota_handle = _ota_handle;
    let _bluetooth_handle = bluetooth_handle;
    let _input = _input;

    let mut server = server;
    let range_proxy_handle = range_proxy_handle;

    loop {
      tokio::select! {
        client_conn = server.listen() => {
          if let Ok((stream, address, mode)) = client_conn
            && let Err(err) = client_man.handle_connection(address, stream, mode, &state).await {
              tracing::error!("failed to accept tcp stream: {:?}", err);
            }
        },
        Ok(msg) = client_listener.recv() => {
          if let Err(err) = client_handler.handle(msg).await {
            tracing::error!("failed to handle websocket message: {:?}", err);
          }
        },
        Some(msg) = bluetooth_rx.recv() => {
          match msg {
            bluetooth::BluetoothEvent::Gateway(data) => {
              if let Err(err) = gateway_handler.handle(data).await {
                tracing::error!("failed to handle bluetooth message: {:?}", err);
              }
            }
          }
        },
        _ = monitoring::wait_for_signal(), if handle_signals => {
          break;
        }
      }
    }

    tracing::info!("shutting down...");
    state.chrome.shutdown().await;
    server.shutdown().await;
    range_proxy_handle.cancel.cancel();

    tracing::info!("thank you for using bridgething!");
  });

  Daemon {
    state: state_out,
    #[cfg(feature = "test-tap")]
    inject: headless_inject,
    #[cfg(feature = "test-tap")]
    server_addrs,
    loop_fut,
  }
}

#[cfg(feature = "test-tap")]
#[derive(Clone)]
pub struct HeadlessInject {
  pub rfcomm: bluetooth::InjectConnectionTx,
  pub iap2: bluetooth::Iap2InjectTx,
}

pub enum BluetoothMode {
  Real,
  #[cfg(feature = "test-tap")]
  Headless,
}

pub struct DaemonConfig {
  pub bluetooth: BluetoothMode,
  pub network_bind: SocketAddr,
  pub stock_bind: SocketAddr,
  pub modern_bind: SocketAddr,
  pub handle_signals: bool,
  pub install_logger: bool,
  pub state_dir: Option<PathBuf>,
  pub webapps_dir: Option<PathBuf>,
  pub ro_webapps_dir: Option<PathBuf>,
}

impl DaemonConfig {
  pub fn real() -> Self {
    Self {
      bluetooth: BluetoothMode::Real,
      network_bind: SocketAddr::from(([0, 0, 0, 0], BRIDGETHING_NETWORK_GATEWAY_PORT)),
      stock_bind: SocketAddr::from(([0, 0, 0, 0], BRIDGETHING_STOCK_WS_PORT)),
      modern_bind: SocketAddr::from(([0, 0, 0, 0], BRIDGETHING_WS_MODERN_PORT)),
      handle_signals: true,
      install_logger: true,
      state_dir: None,
      webapps_dir: None,
      ro_webapps_dir: None,
    }
  }

  #[cfg(feature = "test-tap")]
  pub fn headless(state_dir: PathBuf) -> Self {
    Self {
      bluetooth: BluetoothMode::Headless,
      network_bind: SocketAddr::from(([127, 0, 0, 1], 0)),
      stock_bind: SocketAddr::from(([127, 0, 0, 1], 0)),
      modern_bind: SocketAddr::from(([127, 0, 0, 1], 0)),
      handle_signals: false,
      install_logger: false,
      webapps_dir: Some(state_dir.join("webapps")),
      ro_webapps_dir: Some(state_dir.join("builtin")),
      state_dir: Some(state_dir),
    }
  }
}

fn spawn_ota_event_forwarder(
  bluetooth: bluetooth::BluetoothMan,
  client_man: net::ClientMan,
  mut rx: tokio::sync::mpsc::Receiver<libbridgething::gateway::BridgeToGatewaySystemMsgEvent>,
) {
  use libbridgething::{client::BridgeToClientSystemMsgEvent, gateway::BridgeToGatewaySystemMsgEvent};
  tokio::spawn(async move {
    while let Some(event) = rx.recv().await {
      let client_mirror = match &event {
        BridgeToGatewaySystemMsgEvent::OtaProgress(p) => Some(BridgeToClientSystemMsgEvent::OtaProgress(*p)),
        BridgeToGatewaySystemMsgEvent::OtaError(e) => Some(BridgeToClientSystemMsgEvent::OtaError(e.clone())),
        BridgeToGatewaySystemMsgEvent::DeviceNicknameChanged(_) => None,
      };
      bluetooth.gateway_man.broadcast(event).await;
      if let Some(mirror) = client_mirror {
        let _ = client_man.broadcast_event(mirror).await;
      }
    }
  });
}

fn spawn_nickname_observer(
  mut rx: tokio::sync::watch::Receiver<Option<String>>,
  bluetooth: bluetooth::BluetoothMan,
  bus: net::WireEventBus,
) {
  use libbridgething::{
    client::{BridgeToClientSystemMsgEvent, DeviceNicknameReply as ClientNicknameReply},
    gateway::{BridgeToGatewaySystemMsgEvent, DeviceNicknameReply as GatewayNicknameReply},
  };
  tokio::spawn(async move {
    loop {
      let value = rx.borrow_and_update().clone();

      if let Err(err) = systemd::avahi::publish_bridgething_service(value.as_deref()).await {
        tracing::warn!(?err, "avahi republish on nickname change failed");
      }

      bluetooth
        .gateway_man
        .broadcast(BridgeToGatewaySystemMsgEvent::DeviceNicknameChanged(
          GatewayNicknameReply {
            nickname: value.clone(),
          },
        ))
        .await;

      let client_event = BridgeToClientSystemMsgEvent::DeviceNicknameChanged(ClientNicknameReply { nickname: value });
      if let Err(errs) = bus.broadcast_event(client_event).await {
        tracing::debug!(count = errs.len(), "nickname-change client broadcast non-fatal errors");
      }

      if rx.changed().await.is_err() {
        break;
      }
    }
  });
}

fn trigger_reboot() {
  tokio::spawn(async {
    if let Err(err) = systemd::power::reboot().await {
      tracing::error!("ota reboot failed: {err}");
    }
  });
}

fn trigger_restart_self() {
  tokio::spawn(async {
    if let Err(err) = systemd::power::restart_self().await {
      tracing::error!("ota daemon restart failed: {err}");
    }
  });
}
