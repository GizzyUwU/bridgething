use std::{
  collections::HashMap,
  sync::{Arc, Mutex},
  time::Duration,
};

use bluer::{Address, Session};
use iap2::{Iap2EaGateway, Iap2EaGatewayHandle, Iap2EventsRx, Iap2Handles, Iap2Manager};
use libbridgething::{
  Priority,
  gateway::{BridgeToGatewayMsg, BridgeToGatewayMsgData, GatewayToBridgeMsg, GatewayToBridgeMsgData},
  protocol::EnvelopeProbe,
  wire::{MsgMeta, RequestError, ResponseMeta, WireCommand, WireError, WireEvent, WireRequest},
};
use profiles::ProfileManager;
use tokio::{
  sync::{oneshot, watch},
  task::JoinHandle,
};
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

use ancs::{AncsBootstrap, AncsManager};
use network::NetworkGateway;
pub(crate) use packer::OutboundPacker;
use peer_owners::PeerOwners;
use profiles::ProfileMan;
use rfcomm::RfcommGateway;

use crate::{
  handler::Iap2EventRouter,
  net::{WSError, WireEventBus},
  peer::PeerTracker,
  player::PlayerError,
  state::{DeviceStore, State, StateError, meta::DeviceMeta},
};

pub type BluetoothMan = Arc<BluetoothManager>;
pub type BluetoothTx = tokio::sync::mpsc::Sender<BluetoothEvent>;

const GATEWAY_OUTBOUND_CAPACITY: usize = 64;

#[derive(Debug)]
pub enum BluetoothEvent {
  Gateway(InboundGatewayMessage),
}

#[derive(Debug, Clone)]
pub struct BluetoothDeps {
  pub bus: WireEventBus,
  pub meta: DeviceMeta,
  pub devices: DeviceStore,
  pub peers: PeerTracker,
}

/// Synchronously-available facade for the bluetooth subsystem.
///
/// `BluetoothManager::create()` builds this with every cloneable handle
/// populated (gateway outbound, iAP2 transport/telephony/reconnect,
/// profile_man watch) but *no* bluez activity yet. Consumers wire it
/// into `AppState` and the daemon's HTTP/WS server binds immediately;
/// commands sent through these handles queue in the bounded mpsc until
/// `BluetoothManager::spawn` drains them.
///
/// `spawn` runs the async body (Session/set_powered/profile
/// registration/MFi probe/per-transport drainers/Iap2EventRouter) and
/// is the long-lived bluetooth task. On MFi probe failure the iAP2
/// receivers are dropped here, so sends through the iAP2 handles
/// return `Err(SendError)` for the rest of the run; per-handle wrappers
/// log-and-swallow.
#[derive(Debug)]
pub struct BluetoothManager {
  pub gateway_man: GatewayMan,
  pub iap2: Iap2Handles,
  pub ancs: AncsManager,
  pub profile_man: ProfileManAccess,
}

pub(crate) struct BluetoothBootstrap {
  gateway: GatewayBootstrap,
  iap2_events_rx: Iap2EventsRx,
  iap2_bootstrap: iap2::Iap2Bootstrap,
  ancs: AncsBootstrap,
  profile_man_tx: watch::Sender<Option<ProfileMan>>,
}

impl BluetoothManager {
  pub fn create() -> (BluetoothMan, BluetoothBootstrap) {
    let (gateway_man, gateway_bootstrap) = GatewayMan::allocate();
    let (iap2_handles, iap2_events_rx, iap2_bootstrap) = iap2::allocate_iap2();
    let (ancs_handle, ancs_bootstrap) = AncsManager::allocate();
    let (profile_man_tx, profile_man_rx) = watch::channel(None);

    let manager = Arc::new(Self {
      gateway_man,
      iap2: iap2_handles,
      ancs: ancs_handle,
      profile_man: ProfileManAccess { rx: profile_man_rx },
    });

    let bootstrap = BluetoothBootstrap {
      gateway: gateway_bootstrap,
      iap2_events_rx,
      iap2_bootstrap,
      ancs: ancs_bootstrap,
      profile_man_tx,
    };

    (manager, bootstrap)
  }

  pub fn spawn(
    self: &BluetoothMan,
    bootstrap: BluetoothBootstrap,
    deps: BluetoothDeps,
    state: State,
    bluetooth_tx: BluetoothTx,
  ) -> JoinHandle<()> {
    let manager = self.clone();
    tokio::spawn(async move {
      if let Err(err) = manager.run(bootstrap, deps, state, bluetooth_tx).await {
        tracing::error!(?err, "FATAL: bluetooth coordinator failed");
      }
    })
  }

  async fn run(
    self: BluetoothMan,
    bootstrap: BluetoothBootstrap,
    deps: BluetoothDeps,
    state: State,
    bluetooth_tx: BluetoothTx,
  ) -> BluetoothResult<()> {
    let BluetoothBootstrap {
      gateway,
      mut iap2_events_rx,
      iap2_bootstrap,
      ancs: ancs_bootstrap,
      profile_man_tx,
    } = bootstrap;

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
    let profile_man = Arc::new(ProfileManager::init(
      adapter.clone(),
      deps.bus.clone(),
      deps.devices.clone(),
      deps.peers.clone(),
      self.iap2.reconnect.clone(),
    ));
    let _ = profile_man_tx.send(Some(profile_man.clone()));

    let _agent_handle = auth::build_agent(&session, profile_man.clone()).await?;

    // start stream BEFORE device reconnection attempts
    let _adapter_event_handle = adapter::AdapterEventStream {
      stream: Box::new(adapter.events().await?),
      adapter: adapter.clone(),
    }
    .spawn(profile_man.clone());

    tracing::debug!("setting up bluetooth gateway transports");
    let gateway_runtime = self
      .gateway_man
      .start(gateway, &session, &deps, bluetooth_tx.clone())
      .await?;

    tracing::debug!("setting up iap2 manager");
    let _iap2_handle = Iap2Manager::start(iap2_bootstrap, &session, adapter.clone(), deps.meta.static_meta()).await?;

    tracing::debug!("setting up ancs dispatcher");
    let _ancs_handle = ancs_bootstrap
      .start(adapter.clone(), deps.bus.clone(), self.clone())
      .await;

    let pending_art = state.iap2_pending_art.clone();
    let router = Arc::new(Iap2EventRouter::new(
      state,
      self.clone(),
      profile_man.clone(),
      gateway_runtime.iap2_ea_handle.clone(),
      self.iap2.reconnect.clone(),
      pending_art,
    ));

    loop {
      match iap2_events_rx.recv().await {
        Some(event) => router.route(event).await,
        None => {
          tracing::debug!("bluetooth coordinator: iap2 event stream ended; coordinator parking");
          std::future::pending::<()>().await;
        }
      }
    }
  }

  pub async fn connect(&self, mac: &str) -> bluer::Result<()> {
    let address: Address = mac.parse()?;
    tracing::debug!(%address, "kicking iAP2 reconnect from connect command");
    self.iap2.reconnect.kick(address).await;
    Ok(())
  }
}

#[derive(Debug, Clone)]
pub struct ProfileManAccess {
  rx: watch::Receiver<Option<ProfileMan>>,
}

impl ProfileManAccess {
  pub async fn get(&self) -> ProfileMan {
    let mut rx = self.rx.clone();
    loop {
      if let Some(pm) = rx.borrow_and_update().as_ref() {
        return pm.clone();
      }
      if rx.changed().await.is_err() {
        std::future::pending::<()>().await;
      }
    }
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
  outbound_tx: GatewaySendTx,
  peer_owners: PeerOwners,
  pending: PendingRequests,
}

pub(crate) struct GatewayBootstrap {
  outbound_rx: GatewaySendRx,
}

#[derive(Debug)]
struct GatewayRuntime {
  iap2_ea_handle: Iap2EaGatewayHandle,
  _rfcomm_handle: JoinHandle<()>,
  _rfcomm_listener: JoinHandle<()>,
  _iap2_ea_handle: JoinHandle<()>,
  _network_handle: JoinHandle<()>,
  _router_handle: JoinHandle<()>,
}

impl GatewayMan {
  fn allocate() -> (Self, GatewayBootstrap) {
    let (outbound_tx, outbound_rx) = tokio::sync::mpsc::channel(GATEWAY_OUTBOUND_CAPACITY);
    let me = Self {
      outbound_tx,
      peer_owners: PeerOwners::new(),
      pending: Arc::new(Mutex::new(HashMap::new())),
    };
    (me, GatewayBootstrap { outbound_rx })
  }

  async fn start(
    &self,
    bootstrap: GatewayBootstrap,
    session: &Session,
    deps: &BluetoothDeps,
    bluetooth_tx: BluetoothTx,
  ) -> BluetoothResult<GatewayRuntime> {
    tracing::debug!("initializing bluetooth gateway manager");

    let (rfcomm_recv_tx, rfcomm_recv_rx) = tokio::sync::mpsc::channel(16);
    let (rfcomm_send_tx, rfcomm_send_rx) = tokio::sync::mpsc::channel(16);

    let _rfcomm_handle = RfcommGateway::init(
      session,
      deps.meta.clone(),
      deps.peers.clone(),
      rfcomm_recv_tx,
      rfcomm_send_rx,
      self.peer_owners.clone(),
    )
    .await?
    .spawn();

    let _rfcomm_listener = spawn_gateway_listener(GatewayType::Rfcomm, rfcomm_recv_rx, bluetooth_tx.clone());

    let (iap2_ea, iap2_ea_handle) = Iap2EaGateway::init(
      deps.meta.clone(),
      deps.peers.clone(),
      bluetooth_tx.clone(),
      self.peer_owners.clone(),
    );
    let iap2_ea_send_tx = iap2_ea.send_tx();
    let _iap2_ea_handle_join = iap2_ea.spawn();

    let network = NetworkGateway::init(
      deps.meta.clone(),
      deps.peers.clone(),
      bluetooth_tx.clone(),
      self.peer_owners.clone(),
    )
    .await?;
    let network_send_tx = network.send_tx();
    let _network_handle = network.spawn();

    let _router_handle = spawn_outbound_router(
      bootstrap.outbound_rx,
      self.peer_owners.clone(),
      rfcomm_send_tx,
      iap2_ea_send_tx,
      network_send_tx,
    );

    Ok(GatewayRuntime {
      iap2_ea_handle,
      _rfcomm_handle,
      _rfcomm_listener,
      _iap2_ea_handle: _iap2_ea_handle_join,
      _network_handle,
      _router_handle,
    })
  }

  pub async fn send_all(&self, data: OutboundGatewayMessage) {
    if let Err(err) = self.outbound_tx.send(data).await {
      tracing::error!(?err, "gateway outbound queue closed; drop");
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

fn spawn_gateway_listener(gateway_type: GatewayType, mut rx: GatewayRecvRx, tx: BluetoothTx) -> JoinHandle<()> {
  tracing::debug!("spawning gateway listener for {gateway_type:?}");
  tokio::spawn(async move {
    while let Some(msg) = rx.recv().await {
      if let Err(err) = tx.send(BluetoothEvent::Gateway(msg)).await {
        tracing::error!("failed to send message to bluetooth manager: {:?}", err);
      }
    }
    tracing::error!("gateway connection closed?? this is very very bad!!");
  })
}

fn spawn_outbound_router(
  mut outbound_rx: GatewaySendRx,
  peer_owners: PeerOwners,
  rfcomm_send_tx: GatewaySendTx,
  iap2_ea_send_tx: GatewaySendTx,
  network_send_tx: GatewaySendTx,
) -> JoinHandle<()> {
  tokio::spawn(async move {
    while let Some(msg) = outbound_rx.recv().await {
      let targets = resolve_targets(&peer_owners, msg.address);
      match targets.len() {
        0 => tracing::trace!("outbound router: no targets for {:?}; dropping", msg.address),
        1 => dispatch_to(targets[0], msg, &rfcomm_send_tx, &iap2_ea_send_tx, &network_send_tx).await,
        _ => {
          let last = *targets.last().expect("non-empty targets");
          for kind in &targets[..targets.len() - 1] {
            dispatch_to(*kind, msg.clone(), &rfcomm_send_tx, &iap2_ea_send_tx, &network_send_tx).await;
          }
          dispatch_to(last, msg, &rfcomm_send_tx, &iap2_ea_send_tx, &network_send_tx).await;
        }
      }
    }
    tracing::debug!("outbound router: outbound channel closed; exiting");
  })
}

fn resolve_targets(peer_owners: &PeerOwners, address: Option<Address>) -> Vec<GatewayType> {
  match address {
    Some(addr) => match peer_owners.owner(&addr) {
      Some(kind) => vec![kind],
      None => {
        tracing::trace!(%addr, "outbound router: no transport owns address; dropping");
        Vec::new()
      }
    },
    None => {
      let active = peer_owners.active_kinds();
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

async fn dispatch_to(
  kind: GatewayType,
  msg: OutboundGatewayMessage,
  rfcomm_send_tx: &GatewaySendTx,
  iap2_ea_send_tx: &GatewaySendTx,
  network_send_tx: &GatewaySendTx,
) {
  let tx = match kind {
    GatewayType::Rfcomm => rfcomm_send_tx,
    GatewayType::Iap2Ea => iap2_ea_send_tx,
    GatewayType::Network => network_send_tx,
  };
  if let Err(err) = tx.send(msg).await {
    tracing::error!(?err, ?kind, "outbound router: transport queue closed");
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
