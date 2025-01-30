mod bt;
mod dbus;

mod ble;
mod ws;

mod als;
mod mic;
mod systemd;

mod handler;
mod state;

mod msg;

mod monitoring;

use ble::GatewayCon;
use bt::Bluetooth;
use handler::Handler;
use state::State;
use systemd::Notify;

#[tokio::main]
async fn main() {
  monitoring::init_logger();

  let notifier = systemd::init_notifier();
  let mut state = State::init().await.expect("failed to initialize state!!");

  let server = ws::Server::bind().await.expect("failed to bind to 127.0.0.1:8890");
  let (client_man, mut client_listener) = ws::create_client_manager();

  notifier.status("initializing bluetooth stack...");
  let mut bluetooth = Bluetooth::init(client_man.clone(), &mut state)
    .await
    .expect("failed to initialize the bluetooth stack");

  let mut gateway_con = GatewayCon::init(&bluetooth.adapter)
    .await
    .expect("failed to create gatt server for gateway connections!");

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
      Ok(msg) = client_listener.listen() => {
        if let Err(err) = Handler::new(client_man.clone(), &mut state, &mut bluetooth, msg.id, msg.from, msg.stock_msg_id).handle(msg.data).await {
          tracing::error!("failed to handle websocket message: {:?}", err);
        }
      },
      msg = bluetooth.listen() => {
        if let Err(err) = bluetooth.handle_event(&mut state, msg).await {
          tracing::error!("failed to handle bluetooth message: {:?}", err);
        }
      },
      Some(event) = gateway_con.listen() => {
        tracing::trace!("new gateway event: {:?}", event);
      },
      Some(event) = dbus::maybe_recv(&mut state.player) => {
        if let Some(player) = &mut state.player {
          if let Err(err) = player.handle_event(event).await {
            tracing::error!("failed to handle bluetooth message: {:?}", err);
          }
        }
      },
      _ = monitoring::wait_for_signal() => {
        break;
      }
    }
  }

  tracing::info!("thank you for using bridgething!");
}
