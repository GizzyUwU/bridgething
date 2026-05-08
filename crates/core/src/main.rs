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

use als::{AlsConfig, AlsManager};
use asset::{AssetCache, AssetIngest};
use authority::AuthorityRegistry;
use bluetooth::{BluetoothDeps, BluetoothManager};
use capabilities::CapabilitiesRegistry;
use handler::{ClientHandler, GatewayHandler, Iap2EventRouter};
use mic::{MicConfig, MicManager};
use ota::{OtaOrchestrator, OtaTerminators, RangeProxy};
use peer::PeerTracker;
use player::Player;
use state::{
  AppState, AssembledState, AudioManager, DeviceStore, KvStore, MetaStore, RouteTable, TelephonyManager, TimeManager,
  WebappRegistry,
};
use systemd::Notify;
use transfer::ChunkedTransfer;
use transport::TransportController;

#[tokio::main]
async fn main() {
  let (log_tap, log_tap_layer) = state::LogTap::new();
  monitoring::init_logger(log_tap_layer);

  let notifier = systemd::init_notifier();

  notifier.status("initializing bridgething...");
  let meta = state::meta::SuperbirdMeta::read_or_default().await;
  tracing::debug!("metadata: {:?}", &meta);

  let (client_man, mut client_listener) = net::create_client_manager();
  let bus = net::WireEventBus::new(client_man.clone());

  let db = state::open_state_db().await.expect("failed to open state database");
  let devices = DeviceStore::new(db.clone());
  let kv = KvStore::new(db.clone());
  let meta_store = MetaStore::new(db.clone());

  let webapps = WebappRegistry::init()
    .await
    .expect("failed to initialize webapp registry");
  meta_store
    .enforce_active_webapp_exists(&webapps)
    .await
    .expect("failed to enforce active webapp invariant");

  let asset_pending = AssetCache::init(db.clone(), paths::assets_blobs_dir())
    .await
    .expect("failed to initialize asset cache");
  let (assets, asset_cache_handle) = asset_pending.spawn();

  let transfer_pending = ChunkedTransfer::init(paths::transfers_dir())
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
  let asset_wait = asset::wait::AssetWaitTracker::new();
  let _asset_invalidator = asset::wait::spawn_invalidator(assets.clone(), asset_wait.clone());
  let iap2_pending_art = handler::iap2::Iap2PendingArt::new();
  let peers = PeerTracker::new(
    bus.clone(),
    player.clone(),
    capabilities.clone(),
    ws_routes.clone(),
    stream_routes.clone(),
  );

  let chrome = chrome::Chrome::init().await.expect("failed to initialize chrome");

  notifier.status("initializing bluetooth stack...");
  let (bluetooth_tx, mut bluetooth_rx) = tokio::sync::mpsc::channel(16);
  let bluetooth::BluetoothInit {
    manager: bluetooth,
    mut iap2_events_rx,
  } = BluetoothManager::init(
    BluetoothDeps {
      bus: bus.clone(),
      meta: meta.clone(),
      devices: devices.clone(),
      peers: peers.clone(),
    },
    bluetooth_tx,
  )
  .await
  .expect("failed to initialize bluetooth stack");

  let telephony = TelephonyManager::new(bus.clone(), bluetooth.iap2_telephony_handle());
  let time = TimeManager::new(bus.clone());
  let audio = AudioManager::new(authority.clone(), bus.clone());

  let (als, als_handle) = AlsManager::init(bus.clone(), AlsConfig::default())
    .await
    .expect("failed to initialize ALS manager")
    .spawn();
  let (mic, mic_handle) = MicManager::init(bus.clone(), bluetooth.clone(), MicConfig::default())
    .await
    .spawn();

  let (ancs, _ancs_handle) = bluetooth::ancs::AncsManager::spawn(bus.clone(), bluetooth.clone())
    .await
    .expect("failed to initialize ANCS manager");

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

  let state = AppState::assemble(AssembledState {
    client_man: client_man.clone(),
    bus,
    meta,
    player,
    chrome,
    webapps,
    assets,
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
    ancs,
    db,
    meta_store,
    asset_cache_handle,
    transfer_handle,
    ingest_handle,
    als_handle,
    mic_handle,
  });

  spawn_ota_event_forwarder(bluetooth.clone(), state.client_man.clone(), ota_events_rx);

  let transport = TransportController::new(
    state.authority.clone(),
    state.player.clone(),
    bluetooth.clone(),
    bluetooth.iap2_transport_handle(),
  );

  let iap2_router = bluetooth.iap2_reconnect_handle().map(|reconnect| {
    std::sync::Arc::new(Iap2EventRouter::new(
      state.clone(),
      bluetooth.clone(),
      bluetooth.profile_man.clone(),
      bluetooth.gateway_man.iap2_ea_handle(),
      reconnect,
      state.iap2_pending_art.clone(),
    ))
  });

  notifier.status("initializing server binds...");
  let mut server = net::Server::bind(state.clone(), bluetooth.clone())
    .await
    .expect("failed to bind to 127.0.0.1:8890");

  let client_handler = ClientHandler::new(state.clone(), bluetooth.clone(), transport);
  let gateway_handler = GatewayHandler::new(state.clone(), bluetooth.clone(), ota);

  let _input = input::InputManager::spawn(state.clone());

  if let Err(err) = proxy::spawn(state.clone(), bluetooth.clone()).await {
    tracing::warn!(
      ?err,
      "SOCKS proxy failed to bind; chromium net.proxy webapps will not work"
    );
  }

  notifier.ready(true, Some("ready to accept connections..."));

  loop {
    tokio::select! {
      Ok((stream, address, mode)) = server.listen() => {
        if let Err(err) = client_man.handle_connection(address, stream, mode, &state).await {
          tracing::error!("failed to accept tcp stream: {:?}", err);
          continue;
        };
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
      Some(event) = recv_iap2(&mut iap2_events_rx) => {
        if let Some(router) = iap2_router.as_ref() {
          router.route(event).await;
        }
      },

      _ = monitoring::wait_for_signal() => {
        break;
      }
    }
  }

  tracing::info!("shutting down...");
  state.chrome.shutdown().await;
  server.shutdown().await;
  range_proxy_handle.cancel.cancel();

  tracing::info!("thank you for using bridgething!");
}

async fn recv_iap2(rx: &mut Option<bluetooth::iap2::Iap2EventsRx>) -> Option<bluetooth::iap2::Iap2Event> {
  match rx {
    Some(rx) => rx.recv().await,
    None => std::future::pending().await,
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
      };
      bluetooth.gateway_man.broadcast(event).await;
      if let Some(mirror) = client_mirror {
        let _ = client_man.broadcast_event(mirror).await;
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
