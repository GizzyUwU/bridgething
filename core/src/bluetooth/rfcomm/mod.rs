use std::collections::HashMap;

use bluer::{
  Address, Session,
  rfcomm::{
    self, ConnectRequest, Profile, ProfileHandle, Stream,
    stream::{OwnedReadHalf, OwnedWriteHalf},
  },
};
use futures::StreamExt;
use libbridgething::{BRIDGETHING_PROFILE_UUID, BRIDGETHING_RFCOMM_CHANNEL};
use tokio::{io::AsyncReadExt, task::JoinHandle};

use crate::state::State;

use super::{BluetoothResult, GatewayRecvTx, GatewaySendRx};

type ConnectionTx = tokio::sync::mpsc::Sender<()>;
type ConnectionRx = tokio::sync::mpsc::Receiver<()>;

#[derive(Debug)]
struct Connection {
  writer: OwnedWriteHalf,
  _reader_handle: JoinHandle<()>,
}

impl Connection {
  fn new(stream: Stream, tx: ConnectionTx) -> Self {
    let (reader, writer) = stream.into_split();
    let _reader_handle = tokio::spawn(reader_task(reader, tx));

    Self { writer, _reader_handle }
  }
}

async fn reader_task(mut reader: OwnedReadHalf, tx: ConnectionTx) {
  let mut buf = vec![0; 1024];
  loop {
    match reader.read(&mut buf).await {
      Ok(0) => break, // Connection closed
      Ok(n) => {
        // Handle the data read from the stream
        tracing::debug!("Read {} bytes: {:?}", n, &buf[..n]);
      }
      Err(e) => {
        tracing::error!("Error reading from stream: {:?}", e);
        break;
      }
    }
  }
}

#[derive(Debug)]
pub struct RfcommGateway {
  state: State,
  handle: ProfileHandle,

  conn_tx: ConnectionTx,
  conn_rx: ConnectionRx,
  connections: HashMap<Address, Connection>,

  recv_tx: GatewayRecvTx,
  send_rx: GatewaySendRx,
}

impl RfcommGateway {
  pub async fn init(
    session: &Session,
    state: State,
    recv_tx: GatewayRecvTx,
    send_rx: GatewaySendRx,
  ) -> BluetoothResult<Self> {
    tracing::debug!("creating rfcomm gateway profile");
    let profile = Profile {
      uuid: BRIDGETHING_PROFILE_UUID,
      name: Some("bridgething".to_string()),
      role: Some(rfcomm::Role::Server),
      channel: Some(BRIDGETHING_RFCOMM_CHANNEL as u16),
      require_authentication: Some(false),
      require_authorization: Some(false),
      ..Default::default()
    };

    let handle = session.register_profile(profile).await?;
    let (conn_tx, conn_rx) = tokio::sync::mpsc::channel(16);

    Ok(Self {
      state,
      handle,

      conn_tx,
      conn_rx,
      connections: HashMap::new(),

      recv_tx,
      send_rx,
    })
  }

  pub fn spawn(mut self) -> JoinHandle<()> {
    tokio::spawn(async move {
      if let Err(err) = self.recv().await {
        tracing::error!("rfcomm server died: {:?}", err);
      }
    })
  }

  async fn recv(&mut self) -> BluetoothResult<()> {
    tracing::info!("rfcomm gateway listening for connections");

    loop {
      tokio::select! {
        Some(request) = self.handle.next() => {
          if let Err(err) = self.handle_connect_request(request).await {
            tracing::error!("failed to handle connect request: {:?}", err);
          }
        },
        Some(msg) = self.send_rx.recv() => {
          tracing::trace!("rfcomm gateway received message: {:?}", msg);
          // Handle the message here
        },
        Some(msg) = self.conn_rx.recv() => {
          tracing::trace!("connected device rfcomm message: {:?}", msg);
          // Handle the message here
        },
        else => {
          tracing::error!("rfcomm profile handle stream ended - this should not happen");
          return Ok(());
        }
      }
    }
  }

  async fn handle_connect_request(&mut self, request: ConnectRequest) -> BluetoothResult<()> {
    let address = request.device();
    tracing::debug!("rfcomm connect request from: {address}");

    let stream = request.accept()?;
    tracing::debug!("rfcomm accepted connection from: {address}");

    let connection = Connection::new(stream, self.conn_tx.clone());
    self.connections.insert(address, connection);

    Ok(())
  }
}
