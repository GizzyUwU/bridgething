mod bluetooth;
mod net;

mod als;
mod mic;
mod systemd;

mod asset;
mod authority;
mod chrome;
mod db;

mod handler;
mod ota;
mod paths;
mod peer;
mod player;
mod state;
mod transport;

mod stock;

mod monitoring;

use authority::AuthorityRegistry;
use bluetooth::BluetoothManager;
use chrome::ChromeCommand;
use handler::{ClientHandler, GatewayHandler};
use ota::OtaOrchestrator;
use player::Player;
use state::AppState;
use systemd::Notify;
use transport::TransportController;

#[tokio::main]
async fn main() {
  monitoring::init_logger();

  let notifier = systemd::init_notifier();

  notifier.status("initializing bridgething...");
  let meta = state::meta::SuperbirdMeta::read_or_default().await;
  tracing::debug!("metadata: {:?}", &meta);

  let (client_man, mut client_listener) = net::create_client_manager();
  let authority = AuthorityRegistry::new();
  let player = Player::new(client_man.clone(), authority.clone());

  let chrome = chrome::Chrome::init().await.expect("failed to initialize chrome");

  let is_restart = check_and_mark_restart();
  if is_restart && let Err(e) = chrome.send(ChromeCommand::Reload).await {
    tracing::warn!("failed to queue chrome reload on restart: {:?}", e);
  }

  let state = AppState::init(client_man.clone(), meta, player, chrome, authority)
    .await
    .expect("failed to initialize state!!");

  notifier.status("initializing bluetooth stack...");
  let (bluetooth_tx, mut bluetooth_rx) = tokio::sync::mpsc::channel(16);
  let bluetooth = BluetoothManager::init(state.clone(), bluetooth_tx)
    .await
    .expect("failed to initialize bluetooth stack");
  let transport = TransportController::new(
    state.authority.clone(),
    state.player.clone(),
    bluetooth.clone(),
    bluetooth.iap2_transport_handle(),
  );

  notifier.status("initializing server binds...");
  let mut server = net::Server::bind(state.clone(), bluetooth.clone())
    .await
    .expect("failed to bind to 127.0.0.1:8890");

  let (ota, _ota_handle) = OtaOrchestrator::spawn(
    state.assets.clone(),
    bluetooth.clone(),
    paths::ota_workdir(),
    std::sync::Arc::new(trigger_reboot),
  );

  let client_handler = ClientHandler::new(state.clone(), bluetooth.clone(), transport);
  let gateway_handler = GatewayHandler::new(state.clone(), bluetooth.clone(), ota.clone());

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

      _ = monitoring::wait_for_signal() => {
        break;
      }
    }
  }

  tracing::info!("shutting down...");
  state.chrome.shutdown().await;
  server.shutdown().await;

  tracing::info!("thank you for using bridgething!");
}

/// Triggers a reboot when the OTA orchestrator finishes its `Reboot`
/// phase. In release builds shells out to `sudo reboot`; in debug
/// builds it logs and does nothing so dev hosts don't actually
/// reboot during testing. Sweep to systemd D-Bus tracked separately
/// alongside the existing `client::system::reboot` callsite.
fn trigger_reboot() {
  #[cfg(not(debug_assertions))]
  {
    let status = std::process::Command::new("sh").arg("-c").arg("sudo reboot").status();
    match status {
      Ok(s) if s.success() => {}
      Ok(s) => tracing::error!("ota reboot returned non-zero: {s:?}"),
      Err(err) => tracing::error!("ota reboot failed to spawn: {err:?}"),
    }
  }
  #[cfg(debug_assertions)]
  tracing::warn!("ota reboot requested in debug build - no-op (would reboot in release)");
}

/// Marks the volatile-runtime path that signals "bridgething has run
/// at least once during the current boot." Returns whether the marker
/// was already present (i.e. this is a restart, not the first start
/// since boot). Always leaves the marker in place for the next start.
fn check_and_mark_restart() -> bool {
  let path = paths::restart_marker_path();
  let was_restart = path.exists();
  if let Some(parent) = path.parent()
    && let Err(e) = std::fs::create_dir_all(parent)
  {
    tracing::warn!("failed to create runtime dir {}: {}", parent.display(), e);
    return false;
  }
  if let Err(e) = std::fs::write(&path, b"") {
    tracing::warn!("failed to write restart marker {}: {}", path.display(), e);
  }
  was_restart
}
