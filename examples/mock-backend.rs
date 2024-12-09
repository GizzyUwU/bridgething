use std::net::SocketAddr;

use bridgething::{
  msg::{
    BluetoothRecv, BluetoothSend, Device, DeviceType, RecvMsg, RecvMsgData, SendMsgMeta, StorageRecv, StorageSend,
    SystemSend,
  },
  ws,
};
use uuid::Uuid;

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

  async fn handle(&mut self, ws_msg: RecvMsg) {
    if let RecvMsgData::Hole = ws_msg.data {
      return tracing::trace!("received blackhole message, ignoring");
    };

    tracing::trace!("handling message: {:?}", &ws_msg);

    match ws_msg.data {
      RecvMsgData::Storage(msg) => self.handle_storage_msg(ws_msg.id, ws_msg.from, msg).await,
      RecvMsgData::Bluetooth(msg) => self.handle_bluetooth_msg(ws_msg.id, ws_msg.from, msg).await,
      _ => tracing::warn!("unhandled message!!"),
    }
  }

  async fn handle_storage_msg(&mut self, id: Uuid, address: SocketAddr, msg: StorageRecv) {
    match msg {
      StorageRecv::Get { key, .. } => match key.as_str() {
        "local-storage-data" => {
          self
            .conn_man
            .send(
              id,
              address,
              StorageSend::Response {
                key: "local-storage-data".to_owned(),
                value: Some("{}".to_owned()),
              },
              SendMsgMeta::Response,
            )
            .await
            .expect("failed to send message");
        }
        "onboarding_status" => {
          self
            .conn_man
            .send(
              id,
              address,
              StorageSend::Response {
                key: "onboarding_status".to_owned(),
                value: Some("finished".to_owned()),
              },
              SendMsgMeta::Response,
            )
            .await
            .expect("failed to send message");

          self
            .conn_man
            .send(
              id,
              address,
              SystemSend::__LegacyStockSetupStatus("finished".to_owned()),
              SendMsgMeta::Info,
            )
            .await
            .expect("failed to send message");
        }
        _ => {}
      },
      StorageRecv::Put { key, value, .. } => {}
    };
  }

  async fn handle_bluetooth_msg(&mut self, id: Uuid, address: SocketAddr, msg: BluetoothRecv) {
    let devices = vec![Device {
      name: "Mock Device".to_owned(),
      device_type: DeviceType::Ios,
      mac: "mo::ck::ad::re::ss".to_owned(),
      default: true,
    }];
    match msg {
      BluetoothRecv::List => {
        self
          .conn_man
          .send(
            id,
            address,
            BluetoothSend::PairedDevices(devices),
            SendMsgMeta::Response,
          )
          .await
          .expect("failed to send message");
      }
      BluetoothRecv::Connect { .. } => {
        self
          .conn_man
          .send(
            id,
            address,
            BluetoothSend::Status { connected: true },
            SendMsgMeta::Response,
          )
          .await
          .expect("failed to send message");
        self
          .conn_man
          .send(
            id,
            address,
            BluetoothSend::ConnectedDevice {
              name: devices[0].name.clone(),
              mac: devices[0].mac.clone(),
            },
            SendMsgMeta::Info,
          )
          .await
          .expect("failed to send message");
        self
          .conn_man
          .send(
            id,
            address,
            SystemSend::__LegacyStockRemoteStatus {
              payload: true,
              mac: devices[0].mac.clone(),
              phone_type: devices[0].device_type.clone(),
            },
            SendMsgMeta::Info,
          )
          .await
          .expect("failed to send message");
        self
          .conn_man
          .send(
            id,
            address,
            SystemSend::__LegacyStockTransportStatus { payload: true },
            SendMsgMeta::Info,
          )
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
