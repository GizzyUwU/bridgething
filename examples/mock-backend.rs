use std::net::SocketAddr;

use bridgething::{
  msg::{
    stock::{
      StockBluetoothRecv, StockBluetoothSend, StockConnectionSend, StockDevice, StockDeviceInfo, StockDeviceType,
      StockSetupSend, StockStoragePayload, StockStorageRecv, StockStorageSend,
    },
    AddressedRecvMessage, RecvMessageWithMeta, StockRecv,
  },
  ws,
};

#[tokio::main]
async fn main() {
  init_logger();

  let server = ws::Server::bind().await.expect("failed to bind to 127.0.0.1:8890");
  let mut conn_man = ws::ConnMan::new();

  loop {
    tokio::select! {
      Ok((stream, address)) = server.listen() => {
        if let Err(err) = conn_man.handle_connection(address, stream).await {
          tracing::error!("failed to accept tcp stream: {:?}", err);
          continue;
        };
      },
      Ok(msg) = conn_man.listen() => {
        let mut handler = MockMsgHandler::new(&mut conn_man);
        handler.handle(msg).await;
      },
      _ = wait_for_signal() => {
        tracing::info!("signal received - exiting...");
        break;
      }
    }
  }
}

struct MockMsgHandler<'a> {
  conn_man: &'a mut ws::ConnMan,
}

impl<'a> MockMsgHandler<'a> {
  fn new(conn_man: &'a mut ws::ConnMan) -> Self {
    Self { conn_man }
  }

  async fn handle(&mut self, ws_msg: AddressedRecvMessage) {
    let RecvMessageWithMeta::Stock(msg) = ws_msg.data else {
      return tracing::trace!("ignoring message.");
    };

    if StockRecv::Log != msg {
      tracing::trace!("handling message: {:?}", &msg);
    }

    match msg {
      StockRecv::Storage(msg) => self.handle_storage_msg(ws_msg.from, msg).await,
      StockRecv::Bluetooth(msg) => self.handle_bluetooth_msg(ws_msg.from, msg).await,
      StockRecv::Log => {} // ignore logs
      _ => tracing::warn!("unhandled stock message!!"),
    }
  }

  async fn handle_storage_msg(&mut self, address: SocketAddr, msg: StockStorageRecv) {
    match msg {
      StockStorageRecv::Get { key, .. } => match key.as_str() {
        "local-storage-data" => {
          self
            .conn_man
            .send(
              address,
              StockStorageSend::Response {
                payload: StockStoragePayload {
                  key: "local-storage-data".to_owned(),
                  value: Some("{}".to_owned()),
                  value_type: "string".to_string(),
                  error: None,
                },
              },
            )
            .await
            .expect("failed to send message");
        }
        "onboarding_status" => {
          self
            .conn_man
            .send(
              address,
              StockStorageSend::Response {
                payload: StockStoragePayload {
                  key: "onboarding_status".to_owned(),
                  value: Some("finished".to_owned()),
                  value_type: "string".to_string(),
                  error: None,
                },
              },
            )
            .await
            .expect("failed to send message");

          self
            .conn_man
            .send(
              address,
              StockSetupSend::Status {
                payload: "finished".into(),
              },
            )
            .await
            .expect("failed to send message");
        }
        _ => {}
      },
      StockStorageRecv::Put { key, value, .. } => {}
    };
  }

  async fn handle_bluetooth_msg(&mut self, address: SocketAddr, msg: StockBluetoothRecv) {
    let devices = vec![StockDevice {
      address: "mo::ck::ad::re::ss".to_owned(),
      default: true,
      device_info: StockDeviceInfo {
        name: "Mock Device".to_owned(),
        device_type: StockDeviceType::Ios,
      },
    }];
    match msg {
      StockBluetoothRecv::List => {
        self
          .conn_man
          .send(address, devices)
          .await
          .expect("failed to send message");
      }
      StockBluetoothRecv::Select { .. } => {
        self
          .conn_man
          .send(address, StockBluetoothSend::ConnectionStatus { connected: true })
          .await
          .expect("failed to send message");
        self
          .conn_man
          .send(
            address,
            StockBluetoothSend::CurrentDevice {
              address: devices[0].address.clone(),
              name: devices[0].device_info.name.clone(),
            },
          )
          .await
          .expect("failed to send message");
        self
          .conn_man
          .send(
            address,
            StockConnectionSend::RemoteStatus {
              payload: true,
              mac: devices[0].address.clone(),
              phone_type: devices[0].device_info.device_type.clone(),
            },
          )
          .await
          .expect("failed to send message");
        self
          .conn_man
          .send(address, StockConnectionSend::TransportStatus { payload: true })
          .await
          .expect("failed to send message");
      }
      _ => {}
    };
  }
}

fn init_logger() {
  use tracing::metadata::LevelFilter;
  use tracing_subscriber::{
    filter::Directive, fmt, prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
  };

  #[cfg(debug_assertions)]
  let default_directive = Directive::from(LevelFilter::TRACE);

  #[cfg(debug_assertions)]
  let filter_directives = if let Ok(filter) = std::env::var("RUST_LOG") {
    filter
  } else {
    "mock_backend=trace,bridgething=trace".to_string()
  };

  let filter = EnvFilter::builder()
    .with_default_directive(default_directive)
    .parse_lossy(filter_directives);

  tracing_subscriber::registry()
    .with(
      fmt::layer()
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .with_filter(filter),
    )
    .init();
}

async fn wait_for_signal() {
  use tokio::signal::{
    ctrl_c,
    unix::{signal, SignalKind},
  };

  let mut signal_terminate = signal(SignalKind::terminate()).expect("could not create signal handler");

  tokio::select! {
    _ = signal_terminate.recv() => tracing::info!("received SIGTERM, shutting down"),
    _ = ctrl_c() => tracing::info!("ctrl-c received, shutting down"),
  };
}
