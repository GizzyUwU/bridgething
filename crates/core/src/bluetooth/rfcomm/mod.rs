use std::collections::HashMap;

use bluer::{
  Address, Session,
  rfcomm::{self, ConnectRequest, Profile, ProfileHandle, Stream},
};
use futures::StreamExt;
use libbridgething::{
  BRIDGETHING_PROFILE_UUID, BRIDGETHING_RFCOMM_CHANNEL, PeerCompanionStatus, Priority,
  gateway::{BridgeToGatewayMsg, BridgeToGatewayMsgData, GatewayToBridgeMsg},
  protocol::{BridgeEndec, PrioritizedFrame},
  wire::MsgMeta,
};
use tokio::{
  io::{AsyncWriteExt, ReadHalf, WriteHalf},
  sync::mpsc,
  task::JoinHandle,
};
use tokio_util::{
  bytes::{Bytes, BytesMut},
  codec::{Encoder, FramedRead},
};

use super::{BluetoothResult, GatewayRecvTx, GatewaySendRx, peer_owners::PeerOwners};
use crate::{
  bluetooth::{GatewayType, InboundGatewayMessage, OutboundGatewayMessage, OutboundPacker},
  state::State,
};

/// Soft cap on a single batched write. RFCOMM transparently segments
/// at L2CAP so this is purely about how many small frames the packer
/// coalesces per writer-task tick. Big enough to amortize Normal-Bulk
/// preemption overhead, small enough that the packer doesn't hog the
/// writer task across many milliseconds of throughput.
const RFCOMM_BATCH_BYTES: usize = 4 * 1024;
const LANE_CAPACITY: usize = 16;

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum ConnectionMessage {
  Msg(GatewayToBridgeMsg),
  Close,
}

impl From<GatewayToBridgeMsg> for ConnectionMessage {
  fn from(msg: GatewayToBridgeMsg) -> Self {
    Self::Msg(msg)
  }
}

type ConnectionTx = mpsc::Sender<(Address, ConnectionMessage)>;
type ConnectionRx = mpsc::Receiver<(Address, ConnectionMessage)>;

#[derive(Debug)]
struct Connection {
  address: Address,
  normal_tx: mpsc::Sender<Bytes>,
  bulk_tx: mpsc::Sender<Bytes>,
  _writer_handle: JoinHandle<()>,
  _reader_handle: JoinHandle<()>,
}

impl Connection {
  fn new(address: Address, stream: Stream, tx: ConnectionTx) -> Self {
    let (read_half, write_half) = tokio::io::split(stream);
    let reader = FramedRead::new(read_half, BridgeEndec::default());
    let _reader_handle = tokio::spawn(reader_task(address, reader, tx));

    let (normal_tx, normal_rx) = mpsc::channel(LANE_CAPACITY);
    let (bulk_tx, bulk_rx) = mpsc::channel(LANE_CAPACITY);
    let packer = OutboundPacker::new(normal_rx, bulk_rx, RFCOMM_BATCH_BYTES);
    let _writer_handle = tokio::spawn(writer_task(address, write_half, packer));

    Self {
      address,
      normal_tx,
      bulk_tx,
      _writer_handle,
      _reader_handle,
    }
  }

  async fn send(&self, msg: BridgeToGatewayMsg, priority: Priority) -> BluetoothResult<()> {
    tracing::trace!("({}) sending rfcomm message ({:?}): {:?}", self.address, priority, msg);
    let mut buf = BytesMut::new();
    BridgeEndec::default().encode(PrioritizedFrame { priority, msg }, &mut buf)?;
    let bytes = buf.freeze();
    let lane = match priority {
      Priority::Normal => &self.normal_tx,
      Priority::Bulk => &self.bulk_tx,
    };
    if lane.send(bytes).await.is_err() {
      tracing::debug!("({}) rfcomm writer lane closed; dropping frame", self.address);
    }
    Ok(())
  }
}

async fn reader_task(address: Address, mut reader: FramedRead<ReadHalf<Stream>, BridgeEndec>, tx: ConnectionTx) {
  while let Some(frame) = reader.next().await {
    match frame {
      Ok(frame) => {
        if let Err(e) = tx.send((address, frame.msg.into())).await {
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

async fn writer_task(address: Address, mut writer: WriteHalf<Stream>, mut packer: OutboundPacker) {
  while let Some(batch) = packer.next_batch().await {
    if let Err(err) = writer.write_all(&batch).await {
      tracing::debug!("({address}) rfcomm write error: {:?}", err);
      break;
    }
    if let Err(err) = writer.flush().await {
      tracing::debug!("({address}) rfcomm flush error: {:?}", err);
      break;
    }
  }
  tracing::debug!("({address}) rfcomm writer task exiting");
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
  peer_owners: PeerOwners,
}

impl RfcommGateway {
  pub async fn init(
    session: &Session,
    state: State,
    recv_tx: GatewayRecvTx,
    send_rx: GatewaySendRx,
    peer_owners: PeerOwners,
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
    let (conn_tx, conn_rx) = mpsc::channel(16);

    Ok(Self {
      state,
      handle,

      conn_tx,
      conn_rx,
      connections: HashMap::new(),

      recv_tx,
      send_rx,
      peer_owners,
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
          let OutboundGatewayMessage { address, priority, msg } = data;
          if let Some(address) = address {
            if let Some(conn) = self.connections.get(&address) {
              if let Err(e) = conn.send(msg, priority).await {
                tracing::error!("failed to send rfcomm frame: {:?}", e);
              }
            } else {
              tracing::trace!("rfcomm: no connection for {address}; addressed send dropped");
            }
          } else {
            for conn in self.connections.values() {
              if let Err(e) = conn.send(msg.clone(), priority).await {
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
              self.peer_owners.unregister(address, GatewayType::Rfcomm);
              let _ = self.state.peers.set_companion(address, PeerCompanionStatus::None).await;
            },
            ConnectionMessage::Msg(msg) => {
              if let Err(e) = self.recv_tx.send(InboundGatewayMessage::new(Some(address), GatewayType::Rfcomm, msg)).await {
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

    let connection = Connection::new(address, stream, self.conn_tx.clone());
    connection
      .send(
        BridgeToGatewayMsg {
          id: uuid::Uuid::now_v7(),
          meta: MsgMeta::Event,
          data: BridgeToGatewayMsgData::Version(self.state.meta.clone().into()),
        },
        Priority::Normal,
      )
      .await?;

    self.connections.insert(address, connection);
    self.peer_owners.register(address, GatewayType::Rfcomm);
    let _ = self
      .state
      .peers
      .set_companion(address, PeerCompanionStatus::Pending)
      .await;

    Ok(())
  }
}
