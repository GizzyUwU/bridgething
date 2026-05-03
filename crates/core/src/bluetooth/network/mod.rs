//! WebSocket-based gateway transport. Accepts WS connections on
//! [`BRIDGETHING_NETWORK_GATEWAY_PORT`] and carries the bridgething
//! gateway protocol over `BridgeEndec`-framed binary messages. Mirrors
//! `RfcommGateway`'s shape: per-connection reader/writer pair, a shared
//! `OutboundPacker` for normal+bulk lanes, a single `recv()` loop owns
//! the connection map and drives both inbound dispatch and outbound
//! fan-out.
//!
//! Synthetic addresses: bluetooth peers carry a real MAC; network peers
//! don't, so connection bookkeeping uses a fake address under reserved
//! prefix `0xfe:0xfe:...` with a per-connection counter. PeerTracker /
//! authority routing key by address either way - the prefix keeps these
//! distinguishable from real BlueZ peers without changing any consumer.
//!
//! Intended for dev iteration (host-side reference gateway against a
//! local daemon), with a future real-companion path for Wi-Fi-attached
//! desktops or DeskThing-as-gateway use cases.

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
  BRIDGETHING_NETWORK_GATEWAY_PORT, PeerCompanionStatus, Priority,
  gateway::{BridgeToGatewayMsg, BridgeToGatewayMsgData, GatewayToBridgeMsg},
  protocol::{BridgeEndec, PrioritizedFrame},
  wire::MsgMeta,
};
use tokio::{net::TcpListener, sync::mpsc, task::JoinHandle};
use tokio_util::{
  bytes::{Bytes, BytesMut},
  codec::{Decoder, Encoder},
  sync::CancellationToken,
};

use super::{
  BluetoothEvent, BluetoothResult, BluetoothTx, GatewaySendTx, GatewayType, InboundGatewayMessage,
  OutboundGatewayMessage, OutboundPacker, peer_owners::PeerOwners,
};
use crate::state::State;

/// Soft cap on a single batched WS write. Keeps the packer's
/// Normal-before-Bulk discipline meaningful without one writer task
/// hogging the runtime across many milliseconds of throughput.
const NETWORK_BATCH_BYTES: usize = 16 * 1024;
const LANE_CAPACITY: usize = 16;

/// WebSocket frame + message size cap. Tungstenite defaults to 16 MiB
/// which trips on a single-frame `AssetPush` of even a small `.swu`
/// (~17 MB dev variant). A future asset chunker will let us drop this
/// back; for now the cap is sized for the largest blob a companion is
/// expected to push (full-image `.swu` ~320 MB) with headroom.
const WS_MAX_FRAME_BYTES: usize = 512 * 1024 * 1024;

/// Reserved BT-MAC prefix for synthetic addresses assigned to network
/// peers. Locally-administered (high bit set) and outside any real
/// OUI, so collision with a paired BlueZ peer is impossible.
const NETWORK_ADDR_PREFIX: [u8; 2] = [0xfe, 0xfe];

static NETWORK_ADDR_COUNTER: AtomicU32 = AtomicU32::new(1);

fn next_network_address() -> Address {
  let n = NETWORK_ADDR_COUNTER.fetch_add(1, Ordering::Relaxed).to_be_bytes();
  Address::new([NETWORK_ADDR_PREFIX[0], NETWORK_ADDR_PREFIX[1], n[0], n[1], n[2], n[3]])
}

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

/// Inbound notification posted by the axum WS handler when a fresh
/// peer connects. The recv loop accepts these, mints an Address,
/// spins up the per-connection tasks, and inserts the entry into the
/// connection map. Carrying the WS halves over a channel keeps axum's
/// per-request scope distinct from the gateway recv loop's lifetime.
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

  async fn send(&self, msg: BridgeToGatewayMsg, priority: Priority) -> BluetoothResult<()> {
    tracing::trace!("({}) sending network message ({:?}): {:?}", self.address, priority, msg);
    let mut buf = BytesMut::new();
    BridgeEndec::default().encode(PrioritizedFrame { priority, msg }, &mut buf)?;
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
  let mut decoder = BridgeEndec::default();
  let mut buf = BytesMut::new();
  while let Some(ws_msg) = reader.next().await {
    let ws_msg = match ws_msg {
      Ok(m) => m,
      Err(err) => {
        tracing::debug!("({address}) network ws read error: {:?}", err);
        break;
      }
    };
    let chunk: Bytes = match ws_msg {
      ws::Message::Binary(b) => b,
      ws::Message::Text(_) => {
        tracing::warn!("({address}) network gateway received Text frame; expected Binary, dropping");
        continue;
      }
      ws::Message::Ping(_) | ws::Message::Pong(_) => continue,
      ws::Message::Close(_) => break,
    };
    buf.extend_from_slice(&chunk);

    loop {
      match decoder.decode(&mut buf) {
        Ok(Some(frame)) => {
          if let Err(e) = tx.send((address, frame.msg.into())).await {
            tracing::error!("({address}) failed to forward network gateway message: {:?}", e);
            return;
          }
        }
        Ok(None) => break,
        Err(e) => {
          tracing::debug!("({address}) error decoding network frame: {:?}", e);
          // any subsequent bytes are framing-undefined; drop the connection.
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
  state: State,
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
  pub async fn init(state: State, bluetooth_tx: BluetoothTx, peer_owners: PeerOwners) -> BluetoothResult<Self> {
    tracing::debug!("initializing network gateway on port {BRIDGETHING_NETWORK_GATEWAY_PORT}");

    let (accept_tx, accept_rx) = mpsc::channel::<ConnectAccepted>(16);
    let listener = TcpListener::bind(format!("0.0.0.0:{BRIDGETHING_NETWORK_GATEWAY_PORT}")).await?;
    tracing::info!("network gateway listening on 0.0.0.0:{BRIDGETHING_NETWORK_GATEWAY_PORT}");

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
      state,
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
              let _ = self.state.peers.set_companion(address, PeerCompanionStatus::None).await;
            }
            ConnectionMessage::Msg(msg) => {
              let inbound = InboundGatewayMessage::new(Some(address), GatewayType::Network, msg);
              if let Err(e) = self.bluetooth_tx.send(BluetoothEvent::Gateway(inbound)).await {
                tracing::error!("failed to forward network gateway message: {:?}", e);
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
        if let Err(e) = conn.send(msg, priority).await {
          tracing::error!("failed to send network frame: {:?}", e);
        }
      } else {
        tracing::trace!("network: no connection for {address}; addressed send dropped");
      }
    } else {
      for conn in self.connections.values() {
        if let Err(e) = conn.send(msg.clone(), priority).await {
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
    if let Err(err) = connection
      .send(
        BridgeToGatewayMsg {
          id: uuid::Uuid::now_v7(),
          meta: MsgMeta::Event,
          data: BridgeToGatewayMsgData::Version(self.state.meta.clone().into()),
        },
        Priority::Normal,
      )
      .await
    {
      tracing::warn!("({address}) failed to send initial Version: {err:?}");
    }

    self.connections.insert(address, connection);
    self.peer_owners.register(address, GatewayType::Network);
    let _ = self
      .state
      .peers
      .set_companion(address, PeerCompanionStatus::Pending)
      .await;
  }
}

impl Drop for NetworkGateway {
  fn drop(&mut self) {
    self.cancel_token.cancel();
  }
}
