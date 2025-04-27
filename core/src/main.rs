mod bluetooth;
mod ws;

mod als;
mod mic;
mod systemd;

mod chrome;
mod serve;

mod handler;
mod player;
mod state;

mod msg;

mod monitoring;

use bluetooth::BluetoothManager;
use handler::{ClientHandler, GatewayHandler};
use player::Player;
use state::AppState;
use systemd::Notify;

#[tokio::main]
async fn main() {
  monitoring::init_logger();

  let notifier = systemd::init_notifier();

  let chrome = chrome::Chrome::init().await.expect("failed to initialize chrome");

  notifier.status("initializing websocket server...");
  let server = ws::Server::bind().await.expect("failed to bind to 127.0.0.1:8890");
  let (client_man, mut client_listener) = ws::create_client_manager();

  notifier.status("initializing dbus player manager...");
  let player = Player::new(client_man.clone());
  let state = AppState::init(client_man.clone(), player, chrome)
    .await
    .expect("failed to initialize state!!");

  notifier.status("initializing file server...");
  let serve = serve::FileServe::init(state.clone());

  notifier.status("initializing bluetooth stack...");
  let (bluetooth_tx, mut bluetooth_rx) = tokio::sync::mpsc::channel(16);
  let bluetooth = BluetoothManager::init(state.clone(), bluetooth_tx)
    .await
    .expect("failed to initialize bluetooth stack");

  let client_handler = ClientHandler::new(state.clone(), bluetooth.clone());
  let gateway_handler = GatewayHandler::new(state.clone(), bluetooth.clone());

  notifier.ready(true, Some("ready to accept connections..."));

  loop {
    tokio::select! {
      Ok((stream, address)) = server.listen() => {
        if let Err(err) = client_man.handle_connection(address, stream).await {
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
  serve.shutdown().await;

  tracing::info!("thank you for using bridgething!");
}
