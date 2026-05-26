//! WebSocket-based gateway transport. Accepts WS connections on a
//! caller-supplied bind address and carries the bridgething gateway
//! protocol over `BridgeEndec`-framed binary messages. Per-connection
//! reader/writer pair, a shared `OutboundPacker` for normal+bulk lanes,
//! and a single `recv()` loop owning the connection map and driving both
//! inbound dispatch and outbound fan-out.
//!
//! Synthetic addresses: bluetooth peers carry a real MAC; network peers
//! don't, so connection bookkeeping uses a fake address under reserved
//! prefix `0xfe:0xfe:...` with a per-connection counter. PeerTracker and
//! authority routing key by address either way; the prefix keeps these
//! distinguishable from real BlueZ peers without changing any consumer.

use std::{
  collections::HashMap,
  net::SocketAddr,
  sync::atomic::{AtomicU32, Ordering},
};

use axum::{
  Router,
  extract::{
    ConnectInfo, State as AxumState, WebSocketUpgrade,
    ws::{self, WebSocket},
  },
  response::IntoResponse,
  routing::any,
};
use bluer::Address;
use futures::{
  SinkExt, StreamExt,
  stream::{SplitSink, SplitStream},
};
use libbridgething::{
  Device, DeviceType, PeerCompanionStatus, Priority,
  gateway::{BridgeToGatewayMsg, GatewayToBridgeMsg},
  protocol::{encode_bridge_frame, parse_bridge_frame},
  wire::MsgMeta,
};
use tokio::{net::TcpListener, sync::mpsc, task::JoinHandle};
use tokio_util::{
  bytes::{Bytes, BytesMut},
  sync::CancellationToken,
};

use super::{
  BluetoothEvent, BluetoothResult, BluetoothTx, GatewaySendTx, GatewayType, InboundGatewayMessage,
  OutboundGatewayMessage, OutboundPacker, auto_nack_for_failed_decode, peer_owners::PeerOwners,
};
use crate::{peer::PeerTracker, state::meta::DeviceMeta};

const NETWORK_BATCH_BYTES: usize = 16 * 1024;
const LANE_CAPACITY: usize = 16;
const WS_MAX_FRAME_BYTES: usize = 1024 * 1024;
const SYNTHETIC_NETWORK_ADDR_PREFIX: [u8; 2] = [0xfe, 0xfe];

static NETWORK_ADDR_COUNTER: AtomicU32 = AtomicU32::new(1);

fn next_network_address() -> Address {
  let n = NETWORK_ADDR_COUNTER.fetch_add(1, Ordering::Relaxed).to_be_bytes();
  Address::new([
    SYNTHETIC_NETWORK_ADDR_PREFIX[0],
    SYNTHETIC_NETWORK_ADDR_PREFIX[1],
    n[0],
    n[1],
    n[2],
    n[3],
  ])
}

#[derive(Debug)]
enum ConnectionMessage {
  Msg(Box<GatewayToBridgeMsg>),
  DecodeFailed(libbridgething::protocol::EnvelopeProbe),
  Close,
}

impl From<GatewayToBridgeMsg> for ConnectionMessage {
  fn from(msg: GatewayToBridgeMsg) -> Self {
    Self::Msg(Box::new(msg))
  }
}

type ConnectionTx = mpsc::Sender<(Address, ConnectionMessage)>;
type ConnectionRx = mpsc::Receiver<(Address, ConnectionMessage)>;

struct ConnectAccepted {
  remote: SocketAddr,
  ws: WebSocket,
}

#[derive(Debug)]
struct Connection {
  address: Address,
  remote: SocketAddr,
  normal_tx: mpsc::Sender<Bytes>,
  bulk_tx: mpsc::Sender<Bytes>,
  _writer_handle: JoinHandle<()>,
  _reader_handle: JoinHandle<()>,
}

impl Connection {
  fn new(address: Address, remote: SocketAddr, ws: WebSocket, tx: ConnectionTx) -> Self {
    let (writer, reader) = ws.split();

    let _reader_handle = tokio::spawn(reader_task(address, reader, tx));

    let (normal_tx, normal_rx) = mpsc::channel(LANE_CAPACITY);
    let (bulk_tx, bulk_rx) = mpsc::channel(LANE_CAPACITY);
    let packer = OutboundPacker::new(normal_rx, bulk_rx, NETWORK_BATCH_BYTES);
    let _writer_handle = tokio::spawn(writer_task(address, writer, packer));

    Self {
      address,
      remote,
      normal_tx,
      bulk_tx,
      _writer_handle,
      _reader_handle,
    }
  }

  async fn send(&self, msg: &BridgeToGatewayMsg, priority: Priority) -> BluetoothResult<()> {
    tracing::trace!("({}) sending network message ({:?}): {:?}", self.address, priority, msg);
    let mut buf = BytesMut::new();
    encode_bridge_frame(priority, msg, &mut buf)?;
    let bytes = buf.freeze();
    let lane = match priority {
      Priority::Normal => &self.normal_tx,
      Priority::Bulk => &self.bulk_tx,
    };
    if lane.send(bytes).await.is_err() {
      tracing::debug!("({}) network writer lane closed; dropping frame", self.address);
    }
    Ok(())
  }
}

async fn reader_task(address: Address, mut reader: SplitStream<WebSocket>, tx: ConnectionTx) {
  while let Some(ws_msg) = reader.next().await {
    let ws_msg = match ws_msg {
      Ok(m) => m,
      Err(err) => {
        tracing::debug!("({address}) network ws read error: {:?}", err);
        break;
      }
    };
    let mut chunk: Bytes = match ws_msg {
      ws::Message::Binary(b) => b,
      ws::Message::Text(_) => {
        tracing::warn!("({address}) network gateway received Text frame; expected Binary, dropping");
        continue;
      }
      ws::Message::Ping(_) | ws::Message::Pong(_) => continue,
      ws::Message::Close(_) => break,
    };

    while !chunk.is_empty() {
      match parse_bridge_frame(&mut chunk) {
        Ok(Some(frame)) => {
          if let Err(e) = tx.send((address, frame.msg.into())).await {
            tracing::error!("({address}) failed to forward network gateway message: {:?}", e);
            return;
          }
        }
        Ok(None) => {
          tracing::warn!(
            "({address}) network ws message ended mid-frame ({} byte tail); closing",
            chunk.len()
          );
          return;
        }
        Err(e) if e.is_recoverable() => {
          if let libbridgething::protocol::EndecError::TypedDecode { error, probe } = e {
            tracing::warn!(
              target: "bridgething::network::decode",
              "({address}) typed decode failed: surface={:?} event={:?} kind={:?} id={:?}: {error}",
              probe.data_type, probe.data_event, probe.meta_kind, probe.id,
            );
            if tx
              .send((address, ConnectionMessage::DecodeFailed(*probe)))
              .await
              .is_err()
            {
              tracing::debug!("({address}) network dispatcher gone; dropping decode-failed");
              return;
            }
          }
        }
        Err(e) => {
          tracing::debug!("({address}) error decoding network frame: {:?}", e);
          return;
        }
      }
    }
  }

  tracing::info!("({address}) network connection closed");
  if let Err(e) = tx.send((address, ConnectionMessage::Close)).await {
    tracing::error!("({address}) failed to send close message: {:?}", e);
  }
}

async fn writer_task(address: Address, mut writer: SplitSink<WebSocket, ws::Message>, mut packer: OutboundPacker) {
  while let Some(batch) = packer.next_batch().await {
    if let Err(err) = writer.send(ws::Message::Binary(batch.freeze())).await {
      tracing::debug!("({address}) network ws write error: {:?}", err);
      break;
    }
  }
  let _ = writer.close().await;
  tracing::debug!("({address}) network writer task exiting");
}

#[derive(Clone)]
struct AcceptState {
  tx: mpsc::Sender<ConnectAccepted>,
}

async fn ws_handler(
  ws: WebSocketUpgrade,
  ConnectInfo(remote): ConnectInfo<SocketAddr>,
  AxumState(state): AxumState<AcceptState>,
) -> impl IntoResponse {
  tracing::info!("network gateway: incoming ws upgrade from {remote}");
  ws.max_frame_size(WS_MAX_FRAME_BYTES)
    .max_message_size(WS_MAX_FRAME_BYTES)
    .on_upgrade(move |socket| async move {
      if let Err(err) = state.tx.send(ConnectAccepted { remote, ws: socket }).await {
        tracing::error!("network gateway: failed to enqueue accepted ws: {err:?}");
      }
    })
}

#[derive(Debug)]
pub struct NetworkGateway {
  meta: DeviceMeta,
  peers: PeerTracker,
  bluetooth_tx: BluetoothTx,

  send_tx: GatewaySendTx,
  send_rx: tokio::sync::mpsc::Receiver<OutboundGatewayMessage>,

  conn_tx: ConnectionTx,
  conn_rx: ConnectionRx,
  connections: HashMap<Address, Connection>,
  peer_owners: PeerOwners,

  accept_rx: mpsc::Receiver<ConnectAccepted>,

  cancel_token: CancellationToken,
  _server_handle: JoinHandle<()>,
}

impl NetworkGateway {
  pub async fn init(
    bind: SocketAddr,
    meta: DeviceMeta,
    peers: PeerTracker,
    bluetooth_tx: BluetoothTx,
    peer_owners: PeerOwners,
  ) -> BluetoothResult<Self> {
    tracing::debug!("initializing network gateway on {bind}");

    let (accept_tx, accept_rx) = mpsc::channel::<ConnectAccepted>(16);
    let listener = TcpListener::bind(bind).await?;
    let local_addr = listener.local_addr().unwrap_or(bind);
    tracing::info!("network gateway listening on {local_addr}");

    let cancel_token = CancellationToken::new();
    let app = Router::new()
      .fallback(any(ws_handler))
      .with_state(AcceptState { tx: accept_tx });

    let server_cancel = cancel_token.clone();
    let _server_handle = tokio::spawn(async move {
      tokio::select! {
        res = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()) => {
          if let Err(err) = res {
            tracing::error!("FATAL: network gateway server stopped: {err:?}");
          } else {
            tracing::warn!("network gateway server exited cleanly");
          }
        }
        _ = server_cancel.cancelled() => {
          tracing::debug!("network gateway server shutting down");
        }
      }
    });

    let (conn_tx, conn_rx) = mpsc::channel(16);
    let (send_tx, send_rx) = mpsc::channel(16);

    Ok(Self {
      meta,
      peers,
      bluetooth_tx,

      send_tx,
      send_rx,

      conn_tx,
      conn_rx,
      connections: HashMap::new(),
      peer_owners,

      accept_rx,

      cancel_token,
      _server_handle,
    })
  }

  pub fn send_tx(&self) -> GatewaySendTx {
    self.send_tx.clone()
  }

  pub fn spawn(mut self) -> JoinHandle<()> {
    tokio::spawn(async move { self.recv().await })
  }

  async fn recv(&mut self) {
    tracing::info!("network gateway recv loop active");

    loop {
      tokio::select! {
        Some(accepted) = self.accept_rx.recv() => {
          self.handle_accept(accepted).await;
        }
        Some(data) = self.send_rx.recv() => {
          self.dispatch_outbound(data).await;
        }
        Some((address, msg)) = self.conn_rx.recv() => {
          match msg {
            ConnectionMessage::Close => {
              tracing::debug!("network connection closed: {address}");
              self.connections.remove(&address);
              self.peer_owners.unregister(address, GatewayType::Network);
              let _ = self.peers.remove(address).await;
            }
            ConnectionMessage::Msg(msg) => {
              let inbound = InboundGatewayMessage::new(Some(address), GatewayType::Network, *msg);
              if let Err(e) = self.bluetooth_tx.send(BluetoothEvent::Gateway(inbound)).await {
                tracing::error!("failed to forward network gateway message: {:?}", e);
              }
            }
            ConnectionMessage::DecodeFailed(probe) => {
              if let Some(nack) = auto_nack_for_failed_decode(&probe)
                && let Some(conn) = self.connections.get(&address)
                  && let Err(e) = conn.send(&nack, Priority::Normal).await {
                    tracing::error!("({address}) failed to send auto-nack: {:?}", e);
                  }
            }
          }
        }
        else => {
          tracing::error!("network gateway: all input channels closed - exiting");
          return;
        }
      }
    }
  }

  async fn dispatch_outbound(&self, data: OutboundGatewayMessage) {
    let OutboundGatewayMessage { address, priority, msg } = data;
    if let Some(address) = address {
      if let Some(conn) = self.connections.get(&address) {
        if let Err(e) = conn.send(&msg, priority).await {
          tracing::error!("failed to send network frame: {:?}", e);
        }
      } else {
        tracing::trace!("network: no connection for {address}; addressed send dropped");
      }
    } else {
      for conn in self.connections.values() {
        if let Err(e) = conn.send(&msg, priority).await {
          tracing::error!("failed to send network frame: {:?}", e);
        }
      }
    }
  }

  async fn handle_accept(&mut self, accepted: ConnectAccepted) {
    let address = next_network_address();
    let ConnectAccepted { remote, ws } = accepted;
    tracing::info!("network gateway: accepting connection from {remote} as synthetic {address}");

    let connection = Connection::new(address, remote, ws, self.conn_tx.clone());
    let version = BridgeToGatewayMsg {
      id: uuid::Uuid::now_v7(),
      meta: MsgMeta::Event,
      data: self.meta.snapshot().into(),
    };
    if let Err(err) = connection.send(&version, Priority::Normal).await {
      tracing::warn!("({address}) failed to send initial Version: {err:?}");
    }

    self.connections.insert(address, connection);
    self.peer_owners.register(address, GatewayType::Network);
    let placeholder = Device {
      name: remote.to_string(),
      device_type: DeviceType::Unknown,
      mac: address.to_string(),
      default: false,
    };
    let _ = self.peers.upsert(address, placeholder).await;
    let _ = self.peers.set_companion(address, PeerCompanionStatus::Pending).await;
  }
}

impl Drop for NetworkGateway {
  fn drop(&mut self) {
    self.cancel_token.cancel();
  }
}
