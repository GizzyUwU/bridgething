use std::collections::HashMap;

use bluer::{
  Address, Session,
  rfcomm::{self, ConnectRequest, Profile, ProfileHandle, Stream},
};
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use libbridgething::gateway::{BridgeToGatewayMsg, BridgeToGatewayMsgData, GatewayMsgMeta, GatewayToBridgeMsg};
use libbridgething::protocol::BridgeEndec;
use libbridgething::{BRIDGETHING_PROFILE_UUID, BRIDGETHING_RFCOMM_CHANNEL};
use tokio::task::JoinHandle;
use tokio_util::codec::Framed;

use crate::state::State;

use super::{BluetoothResult, GatewayRecvTx, GatewaySendRx};

type ConnectionTx = tokio::sync::mpsc::Sender<GatewayToBridgeMsg>;
type ConnectionRx = tokio::sync::mpsc::Receiver<GatewayToBridgeMsg>;

#[derive(Debug)]
struct Connection {
  writer: SplitSink<Framed<Stream, BridgeEndec>, BridgeToGatewayMsg>,
  _reader_handle: JoinHandle<()>,
}

impl Connection {
  fn new(stream: Stream, tx: ConnectionTx) -> Self {
    let framed = Framed::new(stream, BridgeEndec::default());
    let (writer, reader) = framed.split();
    let _reader_handle = tokio::spawn(reader_task(reader, tx));
    Self { writer, _reader_handle }
  }

  async fn send(&mut self, msg: BridgeToGatewayMsg) -> BluetoothResult<()> {
    tracing::trace!("sending rfcomm frame: {:?}", msg);
    Ok(self.writer.send(msg).await?)
  }
}

async fn reader_task(mut reader: SplitStream<Framed<Stream, BridgeEndec>>, tx: ConnectionTx) {
  while let Some(frame) = reader.next().await {
    match frame {
      Ok(msg) => {
        if let Err(e) = tx.send(msg).await {
          tracing::error!("failed to forward gateway message: {:?}", e);
        }
      }
      Err(e) => {
        tracing::error!("error decoding rfcomm frame: {:?}", e);
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
          // send bridge message to all connected peers
          for conn in self.connections.values_mut() {
            if let Err(e) = conn.writer.send(msg.clone()).await {
              tracing::error!("failed to send rfcomm frame: {:?}", e);
            }
          }
        },
        Some(msg) = self.conn_rx.recv() => {
          tracing::trace!("connected device rfcomm message: {:?}", msg);
          // forward to application
          let _ = self.recv_tx.send(msg).await;
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

    let mut connection = Connection::new(stream, self.conn_tx.clone());
    connection
      .send(BridgeToGatewayMsg {
        id: uuid::Uuid::now_v7(),
        meta: GatewayMsgMeta::Event,
        data: BridgeToGatewayMsgData::Version(self.state.meta.clone().into()),
      })
      .await?;

    self.connections.insert(address, connection);

    Ok(())
  }
}
