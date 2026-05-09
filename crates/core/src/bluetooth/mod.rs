use std::{
  collections::HashMap,
  sync::{Arc, Mutex},
  time::Duration,
};

use bluer::{Adapter, Address, Session, agent::AgentHandle};
use iap2::{Iap2EaGateway, Iap2EaGatewayHandle, Iap2EventsRx, Iap2Manager, Iap2ReconnectHandle};
use libbridgething::{
  Priority,
  gateway::{BridgeToGatewayMsg, BridgeToGatewayMsgData, GatewayToBridgeMsg, GatewayToBridgeMsgData},
  protocol::EnvelopeProbe,
  wire::{MsgMeta, RequestError, ResponseMeta, WireCommand, WireError, WireEvent, WireRequest},
};
use tokio::{sync::oneshot, task::JoinHandle};
use uuid::Uuid;

// protocol modules
pub mod ancs;
pub mod iap2;
mod network;
pub mod profiles;
mod rfcomm;

// general modules
mod adapter;
mod auth;
#[cfg(debug_assertions)]
mod debug;
mod packer;
mod peer_owners;

use network::NetworkGateway;
pub(crate) use packer::OutboundPacker;
use peer_owners::PeerOwners;
use profiles::ProfileMan;
use rfcomm::RfcommGateway;

use crate::{
  net::{WSError, WireEventBus},
  peer::PeerTracker,
  player::PlayerError,
  state::{DeviceStore, StateError, meta::SuperbirdMeta},
};

pub type BluetoothMan = Arc<BluetoothManager>;
pub type BluetoothTx = tokio::sync::mpsc::Sender<BluetoothEvent>;

#[derive(Debug)]
pub enum BluetoothEvent {
  Gateway(InboundGatewayMessage),
}

#[derive(Debug, Clone)]
pub struct BluetoothDeps {
  pub bus: WireEventBus,
  pub meta: SuperbirdMeta,
  pub devices: DeviceStore,
  pub peers: PeerTracker,
}

#[derive(Debug)]
pub struct BluetoothManager {
  _adapter: Adapter,

  pub profile_man: ProfileMan,
  pub gateway_man: GatewayMan,
  iap2_reconnect: Option<Iap2ReconnectHandle>,
  iap2_transport: Option<iap2::Iap2TransportHandle>,
  iap2_telephony: Option<iap2::Iap2TelephonyHandle>,

  _agent_handle: AgentHandle,
  _iap2_handle: Option<JoinHandle<()>>,
}

#[derive(Debug)]
pub struct BluetoothInit {
  pub manager: BluetoothMan,
  pub iap2_events_rx: Option<Iap2EventsRx>,
}

impl BluetoothManager {
  pub async fn init(deps: BluetoothDeps, tx: BluetoothTx) -> BluetoothResult<BluetoothInit> {
    tracing::debug!("initializing bluetooth manager");
    let session = Session::new().await?;
    let adapter = adapter::get_adapter(&session).await?;

    tracing::debug!("attempting to power on adapter");
    adapter.set_powered(true).await?;

    tracing::info!("initialized bluetooth adapter {}", adapter.name());

    tracing::debug!("configuring adapter");
    adapter.set_pairable_timeout(0).await?;
    adapter.set_pairable(true).await?;

    adapter.set_discoverable_timeout(0).await?;
    adapter.set_discoverable(true).await?;

    #[cfg(debug_assertions)]
    debug::query_adapter(&adapter).await?;

    tracing::debug!("setting up bluetooth profile manager");
    let profile_man = Arc::new(
      profiles::ProfileManager::init(
        adapter.clone(),
        deps.bus.clone(),
        deps.devices.clone(),
        deps.peers.clone(),
      )
      .await,
    );
    let _agent_handle = auth::build_agent(&session, profile_man.clone()).await?;

    // start stream BEFORE device reconnection attempts
    let _adapter_event_handle = adapter::AdapterEventStream {
      stream: Box::new(adapter.events().await?),
      adapter: adapter.clone(),
    }
    .spawn(profile_man.clone());

    tracing::debug!("setting up bluetooth gateway manager");
    let gateway_man = GatewayMan::init(adapter.clone(), &session, &deps, tx.clone()).await?;

    tracing::debug!("setting up iap2 manager");
    let (iap2_reconnect, iap2_transport, iap2_telephony, _iap2_handle, iap2_events_rx) =
      match Iap2Manager::init(&session, adapter.clone(), &deps.meta).await? {
        Some(out) => (
          Some(out.reconnect),
          Some(out.transport),
          Some(out.telephony),
          Some(out.manager.spawn()),
          Some(out.events_rx),
        ),
        None => {
          tracing::info!("iAP2 manager not started (MFi probe failed); native gateway still available");
          (None, None, None, None, None)
        }
      };

    if let Some(handle) = &iap2_reconnect {
      profile_man.set_iap2_reconnect(handle.clone());
    }

    let manager = Arc::new(Self {
      _adapter: adapter,

      profile_man,
      gateway_man,
      iap2_reconnect,
      iap2_transport,
      iap2_telephony,

      _agent_handle,
      _iap2_handle,
    });

    Ok(BluetoothInit {
      manager,
      iap2_events_rx,
    })
  }

  pub fn iap2_transport_handle(&self) -> Option<iap2::Iap2TransportHandle> {
    self.iap2_transport.clone()
  }

  pub fn iap2_telephony_handle(&self) -> Option<iap2::Iap2TelephonyHandle> {
    self.iap2_telephony.clone()
  }

  pub fn iap2_reconnect_handle(&self) -> Option<Iap2ReconnectHandle> {
    self.iap2_reconnect.clone()
  }

  pub async fn connect(&self, mac: &str) -> bluer::Result<()> {
    let address: Address = mac.parse()?;
    if let Some(handle) = &self.iap2_reconnect {
      tracing::debug!(%address, "kicking iAP2 reconnect from connect command");
      handle.kick(address).await;
    } else {
      tracing::debug!(%address, "iAP2 manager not running; connect command has no effect");
    }
    Ok(())
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GatewayType {
  Rfcomm,
  Iap2Ea,
  Network,
}

#[derive(Debug, Clone)]
pub struct InboundGatewayMessage {
  pub address: Option<Address>,
  pub protocol: GatewayType,
  pub priority: Priority,
  pub msg: GatewayToBridgeMsg,
}

impl InboundGatewayMessage {
  pub fn new(address: Option<Address>, protocol: GatewayType, msg: GatewayToBridgeMsg) -> Self {
    Self {
      address,
      protocol,
      priority: Priority::Normal,
      msg,
    }
  }

  pub fn with_priority(mut self, priority: Priority) -> Self {
    self.priority = priority;
    self
  }
}

#[derive(Debug, Clone)]
pub struct OutboundGatewayMessage {
  pub address: Option<Address>,
  pub priority: Priority,
  pub msg: Arc<BridgeToGatewayMsg>,
}

impl OutboundGatewayMessage {
  pub fn new(address: Option<Address>, msg: BridgeToGatewayMsg) -> Self {
    Self {
      address,
      priority: Priority::Normal,
      msg: Arc::new(msg),
    }
  }

  pub fn to(address: Address, msg: BridgeToGatewayMsg) -> Self {
    Self::new(Some(address), msg)
  }

  pub fn all(msg: BridgeToGatewayMsg) -> Self {
    Self::new(None, msg)
  }

  pub fn with_priority(mut self, priority: Priority) -> Self {
    self.priority = priority;
    self
  }

  pub fn bulk(self) -> Self {
    self.with_priority(Priority::Bulk)
  }
}

pub type GatewayRecvTx = tokio::sync::mpsc::Sender<InboundGatewayMessage>;
pub type GatewayRecvRx = tokio::sync::mpsc::Receiver<InboundGatewayMessage>;
pub type GatewaySendTx = tokio::sync::mpsc::Sender<OutboundGatewayMessage>;
pub type GatewaySendRx = tokio::sync::mpsc::Receiver<OutboundGatewayMessage>;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

type PendingRequests = Arc<Mutex<HashMap<Uuid, oneshot::Sender<GatewayToBridgeMsgData>>>>;

#[derive(Debug)]
pub struct GatewayMan {
  rfcomm: GatewayCon,
  iap2_ea_send_tx: GatewaySendTx,
  iap2_ea_handle: Iap2EaGatewayHandle,
  network_send_tx: GatewaySendTx,
  peer_owners: PeerOwners,
  pending: PendingRequests,

  _iap2_ea_handle: JoinHandle<()>,
  _network_handle: JoinHandle<()>,
}

impl GatewayMan {
  pub async fn init(
    adapter: Adapter,
    session: &Session,
    deps: &BluetoothDeps,
    tx: BluetoothTx,
  ) -> BluetoothResult<Self> {
    tracing::debug!("initializing bluetooth gateway manager");
    let peer_owners = PeerOwners::new();

    let rfcomm = GatewayCon::init(
      &adapter,
      session,
      deps.meta.clone(),
      deps.peers.clone(),
      tx.clone(),
      peer_owners.clone(),
    )
    .await?;

    let (iap2_ea, iap2_ea_handle) =
      Iap2EaGateway::init(deps.meta.clone(), deps.peers.clone(), tx.clone(), peer_owners.clone());
    let iap2_ea_send_tx = iap2_ea.send_tx();
    let _iap2_ea_handle = iap2_ea.spawn();

    let network = NetworkGateway::init(deps.meta.clone(), deps.peers.clone(), tx.clone(), peer_owners.clone()).await?;
    let network_send_tx = network.send_tx();
    let _network_handle = network.spawn();

    Ok(Self {
      rfcomm,
      iap2_ea_send_tx,
      iap2_ea_handle,
      network_send_tx,
      peer_owners,
      pending: Arc::new(Mutex::new(HashMap::new())),

      _iap2_ea_handle,
      _network_handle,
    })
  }

  pub fn iap2_ea_handle(&self) -> Iap2EaGatewayHandle {
    self.iap2_ea_handle.clone()
  }

  pub async fn send_all(&self, data: OutboundGatewayMessage) {
    let targets = self.resolve_targets(data.address);
    match targets.len() {
      0 => tracing::trace!("send_all: no targets for {:?}; dropping", data.address),
      1 => self.dispatch_to(targets[0], data).await,
      _ => {
        let last = *targets.last().expect("non-empty targets");
        for kind in &targets[..targets.len() - 1] {
          self.dispatch_to(*kind, data.clone()).await;
        }
        self.dispatch_to(last, data).await;
      }
    }
  }

  fn resolve_targets(&self, address: Option<Address>) -> Vec<GatewayType> {
    match address {
      Some(addr) => match self.peer_owners.owner(&addr) {
        Some(kind) => vec![kind],
        None => {
          tracing::trace!(%addr, "send_all: no transport owns address; dropping");
          Vec::new()
        }
      },
      None => {
        let active = self.peer_owners.active_kinds();
        let mut targets = Vec::with_capacity(3);
        for kind in [GatewayType::Rfcomm, GatewayType::Iap2Ea, GatewayType::Network] {
          if active.contains(&kind) {
            targets.push(kind);
          }
        }
        targets
      }
    }
  }

  async fn dispatch_to(&self, kind: GatewayType, data: OutboundGatewayMessage) {
    match kind {
      GatewayType::Rfcomm => self.rfcomm.send(data).await,
      GatewayType::Iap2Ea => {
        if let Err(err) = self.iap2_ea_send_tx.send(data).await {
          tracing::error!(?err, "failed to enqueue iap2 ea gateway send");
        }
      }
      GatewayType::Network => {
        if let Err(err) = self.network_send_tx.send(data).await {
          tracing::error!(?err, "failed to enqueue network gateway send");
        }
      }
    }
  }

  pub async fn broadcast<E: WireEvent<BridgeToGatewayMsgData>>(&self, event: E) {
    self.broadcast_event_with_priority(event, Priority::Normal).await;
  }

  pub async fn broadcast_event_bulk<E: WireEvent<BridgeToGatewayMsgData>>(&self, event: E) {
    self.broadcast_event_with_priority(event, Priority::Bulk).await;
  }

  async fn broadcast_event_with_priority<E: WireEvent<BridgeToGatewayMsgData>>(&self, event: E, priority: Priority) {
    self
      .send_all(
        OutboundGatewayMessage::all(BridgeToGatewayMsg {
          id: uuid::Uuid::now_v7(),
          meta: MsgMeta::Event,
          data: event.into(),
        })
        .with_priority(priority),
      )
      .await;
  }

  pub async fn send_event<E: WireEvent<BridgeToGatewayMsgData>>(&self, address: Address, event: E) {
    self
      .send_all(OutboundGatewayMessage::to(
        address,
        BridgeToGatewayMsg {
          id: uuid::Uuid::now_v7(),
          meta: MsgMeta::Event,
          data: event.into(),
        },
      ))
      .await;
  }

  pub async fn broadcast_command<C: WireCommand<BridgeToGatewayMsgData>>(&self, cmd: C) {
    self.broadcast_command_with_priority(cmd, Priority::Normal).await;
  }

  pub async fn broadcast_command_bulk<C: WireCommand<BridgeToGatewayMsgData>>(&self, cmd: C) {
    self.broadcast_command_with_priority(cmd, Priority::Bulk).await;
  }

  async fn broadcast_command_with_priority<C: WireCommand<BridgeToGatewayMsgData>>(&self, cmd: C, priority: Priority) {
    self
      .send_all(
        OutboundGatewayMessage::all(BridgeToGatewayMsg {
          id: uuid::Uuid::now_v7(),
          meta: MsgMeta::Command,
          data: cmd.into(),
        })
        .with_priority(priority),
      )
      .await;
  }

  pub async fn send_command<C: WireCommand<BridgeToGatewayMsgData>>(&self, address: Address, cmd: C) {
    self
      .send_all(OutboundGatewayMessage::to(
        address,
        BridgeToGatewayMsg {
          id: uuid::Uuid::now_v7(),
          meta: MsgMeta::Command,
          data: cmd.into(),
        },
      ))
      .await;
  }

  pub async fn request<R: WireRequest<Outbound = BridgeToGatewayMsgData, Inbound = GatewayToBridgeMsgData>>(
    &self,
    address: Option<Address>,
    req: R,
  ) -> Result<R::Response, RequestError<R::DomainError>> {
    self.request_with_priority(address, req, Priority::Normal).await
  }

  pub async fn request_bulk<R: WireRequest<Outbound = BridgeToGatewayMsgData, Inbound = GatewayToBridgeMsgData>>(
    &self,
    address: Option<Address>,
    req: R,
  ) -> Result<R::Response, RequestError<R::DomainError>> {
    self.request_with_priority(address, req, Priority::Bulk).await
  }

  async fn request_with_priority<
    R: WireRequest<Outbound = BridgeToGatewayMsgData, Inbound = GatewayToBridgeMsgData>,
  >(
    &self,
    address: Option<Address>,
    req: R,
    priority: Priority,
  ) -> Result<R::Response, RequestError<R::DomainError>> {
    self
      .request_with_id_priority(Uuid::now_v7(), address, req, priority)
      .await
  }

  pub async fn request_with_id<R: WireRequest<Outbound = BridgeToGatewayMsgData, Inbound = GatewayToBridgeMsgData>>(
    &self,
    id: Uuid,
    address: Option<Address>,
    req: R,
  ) -> Result<R::Response, RequestError<R::DomainError>> {
    self.request_with_id_priority(id, address, req, Priority::Normal).await
  }

  async fn request_with_id_priority<
    R: WireRequest<Outbound = BridgeToGatewayMsgData, Inbound = GatewayToBridgeMsgData>,
  >(
    &self,
    id: Uuid,
    address: Option<Address>,
    req: R,
    priority: Priority,
  ) -> Result<R::Response, RequestError<R::DomainError>> {
    let (tx, rx) = oneshot::channel();
    self
      .pending
      .lock()
      .expect("pending-request map poisoned")
      .insert(id, tx);

    let msg = BridgeToGatewayMsg {
      id,
      meta: MsgMeta::Request,
      data: req.into(),
    };
    self
      .send_all(OutboundGatewayMessage::new(address, msg).with_priority(priority))
      .await;

    match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
      Ok(Ok(data)) => R::extract(data),
      Ok(Err(_)) => {
        self.pending.lock().expect("pending poisoned").remove(&id);
        Err(RequestError::Protocol(WireError::HandlerFailed {
          reason: "response channel closed".into(),
        }))
      }
      Err(_) => {
        self.pending.lock().expect("pending poisoned").remove(&id);
        Err(RequestError::Protocol(WireError::HandlerFailed {
          reason: "request timed out".into(),
        }))
      }
    }
  }

  pub fn complete_pending(&self, request_id: &Uuid, data: GatewayToBridgeMsgData) -> bool {
    let tx = self
      .pending
      .lock()
      .expect("pending-request map poisoned")
      .remove(request_id);
    if let Some(tx) = tx {
      let _ = tx.send(data);
      true
    } else {
      false
    }
  }
}

#[derive(Debug)]
pub struct GatewayCon {
  tx: GatewaySendTx,

  _handle: JoinHandle<()>,
  _listener: JoinHandle<()>,
}

impl GatewayCon {
  pub async fn init(
    _adapter: &Adapter,
    session: &Session,
    meta: SuperbirdMeta,
    peers: PeerTracker,
    bluetooth_tx: BluetoothTx,
    peer_owners: PeerOwners,
  ) -> BluetoothResult<Self> {
    tracing::debug!("initializing rfcomm gateway connection handle");
    let (recv_tx, rx) = tokio::sync::mpsc::channel(16);
    let (tx, notify_rx) = tokio::sync::mpsc::channel(16);

    let _handle = RfcommGateway::init(session, meta, peers, recv_tx, notify_rx, peer_owners)
      .await?
      .spawn();

    let _listener = Self::spawn_listener(GatewayType::Rfcomm, rx, bluetooth_tx);

    Ok(Self { tx, _handle, _listener })
  }

  pub async fn send(&self, data: OutboundGatewayMessage) {
    if let Err(err) = self.tx.send(data).await {
      tracing::error!("failed to send message to gateway: {:?}", err);
    }
  }

  fn spawn_listener(gateway_type: GatewayType, mut rx: GatewayRecvRx, tx: BluetoothTx) -> JoinHandle<()> {
    tracing::debug!("spawning gateway listener for {gateway_type:?}");

    tokio::spawn(async move {
      loop {
        let msg = match rx.recv().await {
          Some(msg) => msg,
          None => {
            tracing::error!("gateway connection closed?? this is very very bad!!");
            return;
          }
        };

        if let Err(err) = tx.send(BluetoothEvent::Gateway(msg)).await {
          tracing::error!("failed to send message to bluetooth manager: {:?}", err);
        }
      }
    })
  }
}

pub fn auto_nack_for_failed_decode(probe: &EnvelopeProbe) -> Option<BridgeToGatewayMsg> {
  if !probe.is_request() {
    return None;
  }
  let request_id = probe.id?;
  Some(BridgeToGatewayMsg {
    id: Uuid::now_v7(),
    meta: MsgMeta::Response(ResponseMeta { request_id }),
    data: BridgeToGatewayMsgData::Error(WireError::Unsupported),
  })
}

pub type BluetoothResult<T> = Result<T, BluetoothError>;
#[derive(Debug, thiserror::Error)]
pub enum BluetoothError {
  #[error("bluez error: {0}")]
  Bluez(#[from] bluer::Error),
  #[error(transparent)]
  WS(#[from] WSError),
  #[error("state error: {0}")]
  State(#[from] StateError),
  #[error("connection to bluetooth daemon timed out")]
  Timeout,
  #[error(transparent)]
  Player(#[from] PlayerError),
  #[error(transparent)]
  MessagePackEnc(#[from] rmp_serde::encode::Error),
  #[error(transparent)]
  MessagePackDec(#[from] rmp_serde::decode::Error),
  #[error(transparent)]
  Endec(#[from] libbridgething::protocol::EndecError),
  #[error(transparent)]
  Io(#[from] std::io::Error),
}

crate::impl_broadcast_failure_from!(BluetoothError);
