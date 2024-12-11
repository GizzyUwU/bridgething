use bridgething::{
  bt::Bluetooth,
  handler::Handler,
  state::State,
  systemd::{self, Notify},
  ws,
};

mod monitoring;

#[tokio::main]
async fn main() {
  monitoring::init_logger();

  let notifier = systemd::init_notifier();
  let mut state = State::init().await.expect("failed to initialize state!!");

  notifier.status("initializing bluetooth stack...");
  let mut bluetooth = Bluetooth::init()
    .await
    .expect("failed to initialize the bluetooth stack");

  let server = ws::Server::bind().await.expect("failed to bind to 127.0.0.1:8890");
  let mut conn_man = ws::ConnMan::new();

  notifier.ready(true, Some("ready to accept connections..."));

  loop {
    tokio::select! {
      Ok((stream, address)) = server.listen() => {
        if let Err(err) = conn_man.handle_connection(address, stream).await {
          tracing::error!("failed to accept tcp stream: {:?}", err);
          continue;
        };
      },
      Ok(msg) = conn_man.listen() => {
        if let Err(err) = Handler::new(&mut conn_man, &mut state, &mut bluetooth, msg.id, msg.from).handle(msg.data).await {
          tracing::error!("failed to handle websocket message: {:?}", err);
        }
      },
      msg = bluetooth.listen() => {
        if let Err(err) = bluetooth.handle_msg(&mut conn_man, &mut state, msg).await {
          tracing::error!("failed to handle bluetooth message: {:?}", err);
        }
      },
      _ = monitoring::wait_for_signal() => {
        break;
      }
    }
  }

  tracing::info!("thank you for using bridgething!");
}
