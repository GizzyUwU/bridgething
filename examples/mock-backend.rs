use std::collections::HashMap;

use bridgething::{
  handler::MsgHandle,
  msg::{BluetoothRecv, BluetoothSend, Device, DeviceType, RecvMsgData, StorageRecv, StorageSend, SystemSend},
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
        MockMsgHandler::new(MsgHandle::new(&mut conn_man, msg.id, msg.from)).handle(msg.data).await;
      },
      _ = wait_for_signal() => {
        tracing::info!("signal received - exiting...");
        break;
      }
    }
  }
}

struct MockMsgHandler<'a> {
  handle: MsgHandle<'a>,
}

impl<'a> MockMsgHandler<'a> {
  fn new(handle: MsgHandle<'a>) -> Self {
    Self { handle }
  }

  async fn handle(&self, data: RecvMsgData) {
    if let RecvMsgData::Hole = data {
      return tracing::trace!("received blackhole message, ignoring");
    };

    tracing::trace!("handling message: {:?}", &data);

    match &data {
      RecvMsgData::Storage(msg) => self.handle_storage_msg(msg).await,
      RecvMsgData::Bluetooth(msg) => self.handle_bluetooth_msg(msg).await,
      _ => tracing::warn!("unhandled message!!"),
    }
  }

  async fn handle_storage_msg(&self, msg: &StorageRecv) {
    match msg {
      StorageRecv::Get { key, .. } => match key.as_str() {
        "local-storage-data" => {
          self
            .handle
            .respond(StorageSend::Response {
              key: "local-storage-data".to_owned(),
              value: Some("{}".to_owned()),
            })
            .await
            .expect("failed to send message");
        }
        "onboarding_status" => {
          self
            .handle
            .respond(StorageSend::Response {
              key: "onboarding_status".to_owned(),
              value: Some("finished".to_owned()),
            })
            .await
            .expect("failed to send message");

          self
            .handle
            .send_info(SystemSend::__LegacyStockSetupStatus("finished".to_owned()))
            .await
            .expect("failed to send message");
        }
        _ => {}
      },
      StorageRecv::Put { key, value, .. } => {
        tracing::debug!("attempting to set storage key: {:?}; value: {:?}", key, value);
      }
      StorageRecv::Delete { key } => {
        tracing::debug!("attempting to delete storage key: {:?}", key);
      }
    };
  }

  async fn handle_bluetooth_msg(&self, msg: &BluetoothRecv) {
    let mock_device = Device {
      name: "Mock Device".to_owned(),
      device_type: DeviceType::Ios,
      mac: "mo::ck::ad::re::ss".to_owned(),
      default: true,
    };
    let devices = HashMap::from_iter(vec![(mock_device.mac.clone(), mock_device.clone())]);

    match msg {
      BluetoothRecv::List => {
        self
          .handle
          .respond(BluetoothSend::PairedDevices(devices))
          .await
          .expect("failed to send message");
      }
      BluetoothRecv::Connect { .. } => {
        self
          .handle
          .respond(BluetoothSend::Status { connected: true })
          .await
          .expect("failed to send message");
        self
          .handle
          .send_info(BluetoothSend::ConnectedDevice {
            name: mock_device.name.clone(),
            mac: mock_device.mac.clone(),
          })
          .await
          .expect("failed to send message");
        self
          .handle
          .send_info(SystemSend::__LegacyStockRemoteStatus {
            payload: true,
            mac: mock_device.mac.clone(),
            phone_type: mock_device.device_type.clone(),
          })
          .await
          .expect("failed to send message");
        self
          .handle
          .send_info(SystemSend::__LegacyStockTransportStatus { payload: true })
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
