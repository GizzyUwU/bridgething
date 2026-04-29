use std::collections::HashMap;

use bluer::{
  Address, Session,
  rfcomm::{self, ConnectRequest, Profile, ProfileHandle, Stream},
};
use futures::{
  SinkExt, StreamExt,
  stream::{SplitSink, SplitStream},
};
use libbridgething::{
  BRIDGETHING_PROFILE_UUID, BRIDGETHING_RFCOMM_CHANNEL,
  gateway::{BridgeToGatewayMsg, BridgeToGatewayMsgData, GatewayMsgMeta, GatewayToBridgeMsg},
  protocol::BridgeEndec,
  server::GatewayStatus,
};
use tokio::task::JoinHandle;
use tokio_util::codec::Framed;

use crate::{
  bluetooth::{GatewayMessage, GatewayType},
  state::State,
};

use super::{BluetoothResult, GatewayRecvTx, GatewaySendRx};

#[derive(Debug)]
enum ConnectionMessage {
  Msg(GatewayToBridgeMsg),
  Close,
}

impl From<GatewayToBridgeMsg> for ConnectionMessage {
  fn from(msg: GatewayToBridgeMsg) -> Self {
    Self::Msg(msg)
  }
}

type ConnectionTx = tokio::sync::mpsc::Sender<(Address, ConnectionMessage)>;
type ConnectionRx = tokio::sync::mpsc::Receiver<(Address, ConnectionMessage)>;

#[derive(Debug)]
struct Connection {
  address: Address,
  writer: SplitSink<Framed<Stream, BridgeEndec>, BridgeToGatewayMsg>,
  _reader_handle: JoinHandle<()>,
}

impl Connection {
  fn new(address: Address, stream: Stream, tx: ConnectionTx) -> Self {
    let framed = Framed::new(stream, BridgeEndec::default());
    let (writer, reader) = framed.split();
    let _reader_handle = tokio::spawn(reader_task(address, reader, tx));
    Self {
      address,
      writer,
      _reader_handle,
    }
  }

  async fn send(&mut self, msg: BridgeToGatewayMsg) -> BluetoothResult<()> {
    tracing::trace!("({}) sending rfcomm message: {:?}", self.address, msg);
    Ok(self.writer.send(msg).await?)
  }
}

async fn reader_task(address: Address, mut reader: SplitStream<Framed<Stream, BridgeEndec>>, tx: ConnectionTx) {
  while let Some(frame) = reader.next().await {
    match frame {
      Ok(msg) => {
        if let Err(e) = tx.send((address, msg.into())).await {
          tracing::error!("({address}) failed to forward gateway message: {:?}", e);
        }
      }
      Err(e) => {
        tracing::debug!("({address}) error decoding rfcomm frame: {:?}", e);
        break;
      }
    }
  }

  tracing::info!("({address}) bluetooth connection closed");
  if let Err(e) = tx.send((address, ConnectionMessage::Close)).await {
    tracing::error!("({address}) failed to send close message: {:?}", e);
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
    tokio::spawn(async move { self.recv().await })
  }

  async fn recv(&mut self) {
    tracing::info!("rfcomm gateway listening for connections");

    loop {
      tokio::select! {
        Some(request) = self.handle.next() => {
          if let Err(err) = self.handle_connect_request(request).await {
            tracing::error!("failed to handle connect request: {:?}", err);
          }
        },
        Some(data) = self.send_rx.recv() => {
          tracing::trace!("rfcomm gateway received message: {:?}", data);
          if let Some(address) = data.address {
            if let Some(conn) = self.connections.get_mut(&address) {
              if let Err(e) = conn.send(data.msg).await {
                tracing::error!("failed to send rfcomm frame: {:?}", e);
              }
            } else {
              tracing::warn!("rfcomm connection not found for address: {:?}", address);
            }
          } else {
            // send bridge message to all connected peers
            for conn in self.connections.values_mut() {
              if let Err(e) = conn.writer.send(data.msg.clone()).await {
                tracing::error!("failed to send rfcomm frame: {:?}", e);
              }
            }
          }
        },
        Some((address, msg)) = self.conn_rx.recv() => {
          tracing::trace!("rfcomm message from {}: {:?}", address, msg);
          match msg {
            ConnectionMessage::Close => {
              tracing::debug!("rfcomm connection closed: {:?}", address);
              self.connections.remove(&address);

              if self.state.gateway_status().await.address == address.to_string() {
                tracing::debug!("\"current\" rfcomm connection closed - setting gateway status to disconnected");
                if let Err(e) = self.state.set_gateway_status(GatewayStatus::default()).await {
                  tracing::error!("failed to set gateway status: {:?}", e);
                }
              }
            },
            ConnectionMessage::Msg(msg) => {
              if let Err(e) = self.recv_tx.send(GatewayMessage::new(Some(address), GatewayType::Rfcomm, msg)).await {
                tracing::error!("failed to send rfcomm message to gateway: {:?}", e);
              }
            }
          }
        },
        else => {
          tracing::error!("rfcomm profile handle stream ended - this should not happen");
          return;
        }
      }
    }
  }

  async fn handle_connect_request(&mut self, request: ConnectRequest) -> BluetoothResult<()> {
    let address = request.device();
    tracing::debug!("rfcomm connect request from: {address}");

    let stream = request.accept()?;
    tracing::debug!("rfcomm accepted connection from: {address}");

    let mut connection = Connection::new(address, stream, self.conn_tx.clone());
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
