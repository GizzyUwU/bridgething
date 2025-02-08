mod bt;

mod ble;
mod ws;

mod als;
mod mic;
mod systemd;

mod handler;
mod player;
mod state;

mod msg;

mod monitoring;

use ble::GatewayCon;
use bt::BluetoothMan;
use handler::ClientHandler;
use player::Player;
use state::AppState;
use systemd::Notify;

#[tokio::main]
async fn main() {
  monitoring::init_logger();

  let notifier = systemd::init_notifier();

  let server = ws::Server::bind().await.expect("failed to bind to 127.0.0.1:8890");
  let (client_man, mut client_listener) = ws::create_client_manager();

  let player = Player::new(client_man.clone());
  let state = AppState::init(client_man.clone(), player)
    .await
    .expect("failed to initialize state!!");

  notifier.status("initializing bluetooth stack...");
  let (bluetooth, mut bt_listener) = BluetoothMan::init(state.clone())
    .await
    .expect("failed to initialize the bluetooth stack");

  let mut gateway_con = GatewayCon::init(&bluetooth.adapter)
    .await
    .expect("failed to create gatt server for gateway connections!");

  let client_handler = ClientHandler::new(state.clone(), bluetooth.clone());

  notifier.ready(true, Some("ready to accept connections..."));

  // TODO: handle all events on spawned threads
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
      msg = bt_listener.recv() => {
        if let Err(err) = bluetooth.handle_event(msg).await {
          tracing::error!("failed to handle bluetooth message: {:?}", err);
        }
      },
      Some(event) = gateway_con.listen() => {
        tracing::trace!("new gateway event: {:?}", event);
      },

      _ = monitoring::wait_for_signal() => {
        break;
      }
    }
  }

  tracing::info!("thank you for using bridgething!");
}
