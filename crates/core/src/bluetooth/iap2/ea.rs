//! Bridge between iAP2 EA streams (per-peer, opened by iOS via
//! `StartExternalAccessoryProtocolSession`) and the modern bridgething
//! gateway protocol surface. From the gateway handler's perspective an
//! EA stream looks identical to a `RfcommGateway` connection: a
//! peer-addressed byte pipe carrying `BridgeEndec`-framed messages,
//! announced with a `Version` event on open and torn down on close.
//!
//! The iAP2 manager calls [`Iap2EaGatewayHandle::notify_open`] on
//! every `SessionEvent::EaStreamOpened`; the gateway task then spawns
//! a per-stream reader/writer pair that wraps the iap2 byte channels
//! with `BridgeEndec`. Outbound `OutboundGatewayMessage`s coming from
//! the handler ride the existing priority byte through the iap2
//! chunker - Bulk frames yield to Normal at chunk boundaries inside
//! the iap2 crate.

use std::collections::HashMap;

use bluer::Address;
use bridgething_iap2::session::{EaPriority, EaStreamSender};
use libbridgething::{
  PeerCompanionStatus, Priority,
  gateway::BridgeToGatewayMsg,
  protocol::{BridgeEndec, encode_bridge_frame},
  wire::MsgMeta,
};
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::{
  bytes::{Bytes, BytesMut},
  codec::Decoder,
};

use super::super::BluetoothResult;
use crate::{
  bluetooth::{
    BluetoothEvent, GatewayType, InboundGatewayMessage, OutboundGatewayMessage, auto_nack_for_failed_decode,
    peer_owners::PeerOwners,
  },
  peer::PeerTracker,
  state::meta::SuperbirdMeta,
};

const STREAM_INPUT_CAPACITY: usize = 16;

/// Notification posted by the iap2 manager when iOS opens a fresh EA
/// stream on a connected peer. Carries the byte channels the iap2
/// crate exposes; the gateway task wraps them in `BridgeEndec` and
/// drives the modern wire protocol on top.
pub struct StreamOpened {
  pub address: Address,
  pub stream_id: u16,
  pub protocol_id: u8,
  pub inbound_rx: mpsc::Receiver<Bytes>,
  pub outbound: EaStreamSender,
}

impl std::fmt::Debug for StreamOpened {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("StreamOpened")
      .field("address", &self.address)
      .field("stream_id", &self.stream_id)
      .field("protocol_id", &self.protocol_id)
      .finish()
  }
}

#[derive(Debug)]
pub struct StreamClosed {
  pub address: Address,
  pub stream_id: u16,
}

/// Public-facing handle the iap2 manager hands its observe loop. The
/// gateway task owns the receiving side and is the only consumer.
#[derive(Clone, Debug)]
pub struct Iap2EaGatewayHandle {
  open_tx: mpsc::Sender<StreamOpened>,
  closed_tx: mpsc::Sender<StreamClosed>,
}

impl Iap2EaGatewayHandle {
  pub async fn notify_open(&self, opened: StreamOpened) {
    if let Err(err) = self.open_tx.send(opened).await {
      tracing::warn!(?err, "iap2 ea gateway: open notification dropped");
    }
  }

  pub async fn notify_closed(&self, closed: StreamClosed) {
    if let Err(err) = self.closed_tx.send(closed).await {
      tracing::warn!(?err, "iap2 ea gateway: close notification dropped");
    }
  }
}

type Key = (Address, u16);

#[derive(Debug)]
struct StreamConn {
  outbound: EaStreamSender,
  _reader_handle: JoinHandle<()>,
}

#[derive(Debug)]
pub struct Iap2EaGateway {
  meta: SuperbirdMeta,
  peers: PeerTracker,
  bluetooth_tx: mpsc::Sender<BluetoothEvent>,
  send_tx: mpsc::Sender<OutboundGatewayMessage>,
  send_rx: mpsc::Receiver<OutboundGatewayMessage>,
  open_rx: mpsc::Receiver<StreamOpened>,
  closed_rx: mpsc::Receiver<StreamClosed>,
  conn_close_tx: mpsc::Sender<Key>,
  conn_close_rx: mpsc::Receiver<Key>,
  conns: HashMap<Key, StreamConn>,
  peer_owners: PeerOwners,
}

impl Iap2EaGateway {
  pub fn init(
    meta: SuperbirdMeta,
    peers: PeerTracker,
    bluetooth_tx: mpsc::Sender<BluetoothEvent>,
    peer_owners: PeerOwners,
  ) -> (Self, Iap2EaGatewayHandle) {
    let (send_tx, send_rx) = mpsc::channel(STREAM_INPUT_CAPACITY);
    let (open_tx, open_rx) = mpsc::channel(STREAM_INPUT_CAPACITY);
    let (closed_tx, closed_rx) = mpsc::channel(STREAM_INPUT_CAPACITY);
    let (conn_close_tx, conn_close_rx) = mpsc::channel(STREAM_INPUT_CAPACITY);
    let handle = Iap2EaGatewayHandle { open_tx, closed_tx };
    let gateway = Self {
      meta,
      peers,
      bluetooth_tx,
      send_tx,
      send_rx,
      open_rx,
      closed_rx,
      conn_close_tx,
      conn_close_rx,
      conns: HashMap::new(),
      peer_owners,
    };
    (gateway, handle)
  }

  pub fn send_tx(&self) -> mpsc::Sender<OutboundGatewayMessage> {
    self.send_tx.clone()
  }

  pub fn spawn(mut self) -> JoinHandle<()> {
    tokio::spawn(async move { self.run().await })
  }

  async fn run(&mut self) {
    tracing::info!("iap2 ea gateway: running");
    loop {
      tokio::select! {
        Some(opened) = self.open_rx.recv() => {
          if let Err(err) = self.handle_open(opened).await {
            tracing::warn!(?err, "iap2 ea gateway: failed to open stream");
          }
        }
        Some(closed) = self.closed_rx.recv() => {
          self.tear_down((closed.address, closed.stream_id)).await;
        }
        Some(key) = self.conn_close_rx.recv() => {
          self.tear_down(key).await;
        }
        Some(msg) = self.send_rx.recv() => {
          self.dispatch_outbound(msg).await;
        }
        else => {
          tracing::error!("iap2 ea gateway: all input channels closed");
          return;
        }
      }
    }
  }

  async fn handle_open(&mut self, opened: StreamOpened) -> BluetoothResult<()> {
    let StreamOpened {
      address,
      stream_id,
      protocol_id,
      inbound_rx,
      outbound,
    } = opened;
    let key = (address, stream_id);
    tracing::info!(%address, stream_id, protocol_id, "iap2 ea gateway: stream opened");

    let _reader_handle = tokio::spawn(reader_task(
      address,
      inbound_rx,
      self.bluetooth_tx.clone(),
      self.conn_close_tx.clone(),
      key,
      outbound.clone(),
    ));
    self.conns.insert(
      key,
      StreamConn {
        outbound,
        _reader_handle,
      },
    );

    let version = BridgeToGatewayMsg {
      id: uuid::Uuid::now_v7(),
      meta: MsgMeta::Event,
      data: self.meta.clone().into(),
    };
    self.send_to_stream(key, &version, Priority::Normal).await;

    self.peer_owners.register(address, GatewayType::Iap2Ea);
    let _ = self.peers.set_companion(address, PeerCompanionStatus::Pending).await;
    Ok(())
  }

  async fn dispatch_outbound(&mut self, message: OutboundGatewayMessage) {
    let OutboundGatewayMessage { address, priority, msg } = message;
    if let Some(address) = address {
      let keys: Vec<Key> = self.conns.keys().copied().filter(|(a, _)| *a == address).collect();
      if keys.is_empty() {
        tracing::trace!(%address, "iap2 ea gateway: no stream for {address}; addressed send dropped");
        return;
      }
      for key in keys {
        self.send_to_stream(key, &msg, priority).await;
      }
    } else {
      let keys: Vec<Key> = self.conns.keys().copied().collect();
      for key in keys {
        self.send_to_stream(key, &msg, priority).await;
      }
    }
  }

  async fn send_to_stream(&mut self, key: Key, msg: &BridgeToGatewayMsg, priority: Priority) {
    let Some(conn) = self.conns.get(&key) else { return };
    let mut buf = BytesMut::new();
    if let Err(err) = encode_bridge_frame(priority, msg, &mut buf) {
      tracing::error!(stream_id = key.1, ?err, "iap2 ea gateway: encode failed");
      return;
    }
    let ea_priority = match priority {
      Priority::Normal => EaPriority::Normal,
      Priority::Bulk => EaPriority::Bulk,
    };
    if let Err(err) = conn.outbound.send(ea_priority, buf.freeze()).await {
      tracing::warn!(stream_id = key.1, ?err, "iap2 ea gateway: chunker channel closed");
      self.tear_down(key).await;
    }
  }

  async fn tear_down(&mut self, key: Key) {
    if self.conns.remove(&key).is_none() {
      return;
    }
    tracing::info!(address = %key.0, stream_id = key.1, "iap2 ea gateway: stream torn down");
    let still_open_for_address = self.conns.keys().any(|(a, _)| *a == key.0);
    if !still_open_for_address {
      self.peer_owners.unregister(key.0, GatewayType::Iap2Ea);
      let _ = self.peers.set_companion(key.0, PeerCompanionStatus::None).await;
    }
  }
}

async fn reader_task(
  address: Address,
  mut inbound_rx: mpsc::Receiver<Bytes>,
  bluetooth_tx: mpsc::Sender<BluetoothEvent>,
  conn_close_tx: mpsc::Sender<Key>,
  key: Key,
  outbound: EaStreamSender,
) {
  let mut buf = BytesMut::new();
  let mut codec = BridgeEndec::default();
  loop {
    loop {
      match codec.decode(&mut buf) {
        Ok(Some(frame)) => {
          let event = BluetoothEvent::Gateway(InboundGatewayMessage::new(
            Some(address),
            GatewayType::Iap2Ea,
            frame.msg,
          ));
          if bluetooth_tx.send(event).await.is_err() {
            tracing::error!(%address, "iap2 ea gateway: bluetooth bus closed");
            let _ = conn_close_tx.send(key).await;
            return;
          }
        }
        Ok(None) => break,
        Err(err) if err.is_recoverable() => {
          if let libbridgething::protocol::EndecError::TypedDecode { error, probe } = err {
            tracing::warn!(
              target: "bridgething::iap2_ea::decode",
              %address, stream_id = key.1,
              "typed decode failed: surface={:?} event={:?} kind={:?} id={:?}: {error}",
              probe.data_type, probe.data_event, probe.meta_kind, probe.id,
            );
            if let Some(nack) = auto_nack_for_failed_decode(&probe) {
              let mut nack_buf = BytesMut::new();
              if let Err(e) = encode_bridge_frame(Priority::Normal, &nack, &mut nack_buf) {
                tracing::error!(%address, ?e, "iap2 ea gateway: encode auto-nack failed");
              } else if let Err(e) = outbound.send(EaPriority::Normal, nack_buf.freeze()).await {
                tracing::warn!(%address, ?e, "iap2 ea gateway: auto-nack send failed");
              }
            }
          }
        }
        Err(err) => {
          tracing::debug!(%address, ?err, "iap2 ea gateway: decode error; tearing down stream");
          let _ = conn_close_tx.send(key).await;
          return;
        }
      }
    }

    match inbound_rx.recv().await {
      Some(chunk) => buf.extend_from_slice(&chunk),
      None => {
        tracing::debug!(%address, "iap2 ea gateway: inbound channel closed");
        let _ = conn_close_tx.send(key).await;
        return;
      }
    }
  }
}
