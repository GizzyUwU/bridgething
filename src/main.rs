use bridgething::{
  ble,
  systemd::{self, Notify},
  ws,
};

mod monitoring;

#[tokio::main]
async fn main() {
  monitoring::init_logger();

  #[cfg(feature = "systemd")]
  let notifier = systemd::SystemdNotify::new();

  #[cfg(not(feature = "systemd"))]
  let notifier = systemd::DummyNotify::new();

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
        // leaving this here...
      },
      _ = monitoring::wait_for_signal() => {
        tracing::info!("signal received - exiting...");
        break;
      }
    }
  }
}
