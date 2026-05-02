use std::sync::Arc;

use ble::GattServer;
use bluer::{Adapter, Address, Session, agent::AgentHandle};
use iap2::{Iap2EaGateway, Iap2EaGatewayHandle, Iap2Manager, Iap2ReconnectHandle};
use libbridgething::{
  ForwardMessage, Priority,
  gateway::{BridgeToGatewayMsg, GatewayMsgMeta, GatewayToBridgeMsg},
};
use tokio::task::JoinHandle;

// protocol modules
mod ble;
pub mod iap2;
mod profiles;
mod rfcomm;

// general modules
mod adapter;
mod auth;
#[cfg(debug_assertions)]
mod debug;
mod packer;

pub(crate) use packer::OutboundPacker;
use profiles::ProfileMan;
use rfcomm::RfcommGateway;

use crate::{
  http::WSError,
  player::PlayerError,
  state::{State, StateError},
};

pub type BluetoothMan = Arc<BluetoothManager>;
pub type BluetoothTx = tokio::sync::mpsc::Sender<BluetoothEvent>;

#[derive(Debug)]
pub enum BluetoothEvent {
  Gateway(InboundGatewayMessage),
}

#[derive(Debug)]
pub struct BluetoothManager {
  session: Session,
  pub adapter: Adapter,
  state: State,
  tx: BluetoothTx,

  pub profile_man: ProfileMan,
  pub gateway_man: GatewayMan,
  iap2_reconnect: Option<Iap2ReconnectHandle>,
  iap2_transport: Option<iap2::Iap2TransportHandle>,

  _agent_handle: AgentHandle,
  _iap2_handle: Option<JoinHandle<()>>,
}

impl BluetoothManager {
  pub async fn init(state: State, tx: BluetoothTx) -> BluetoothResult<BluetoothMan> {
    tracing::debug!("initializing bluetooth manager");
    let session = Session::new().await?;
    let adapter = adapter::get_adapter(&session).await?;

    tracing::debug!("attempting to power on adapter");
    adapter.set_powered(true).await?;

    tracing::info!("initialized bluetooth adapter {}", adapter.name());

    tracing::debug!("configuring adapter");
    adapter.set_pairable_timeout(0).await?;
    adapter.set_pairable(true).await?;

    // TODO: what am i supposed to do here?
    adapter.set_discoverable_timeout(0).await?;
    adapter.set_discoverable(true).await?;

    #[cfg(debug_assertions)]
    debug::query_adapter(&adapter).await?;

    tracing::debug!("setting up bluetooth profile manager");
    let profile_man = Arc::new(profiles::ProfileManager::init(adapter.clone(), state.clone(), tx.clone()).await);
    let _agent_handle = auth::build_agent(&session, profile_man.clone()).await?;

    // start stream BEFORE device reconnection attempts
    let _adapter_event_handle =
      adapter::AdapterEventStream(Box::new(adapter.events().await?)).spawn(profile_man.clone());

    tracing::debug!("setting up bluetooth gateway manager");
    let gateway_man = GatewayMan::init(adapter.clone(), &session, state.clone(), tx.clone()).await?;

    tracing::debug!("setting up iap2 manager");
    let (iap2_reconnect, iap2_transport, _iap2_handle) = match Iap2Manager::init(
      &session,
      adapter.clone(),
      &state,
      profile_man.clone(),
      gateway_man.iap2_ea_handle(),
    )
    .await?
    {
      Some((manager, reconnect_handle, transport_handle)) => (
        Some(reconnect_handle),
        Some(transport_handle),
        Some(manager.spawn()),
      ),
      None => {
        tracing::info!("iAP2 manager not started (MFi probe failed); native gateway still available");
        (None, None, None)
      }
    };

    Ok(Arc::new(Self {
      session,
      adapter,
      state,

      tx,

      profile_man,
      gateway_man,
      iap2_reconnect,
      iap2_transport,

      _agent_handle,
      _iap2_handle,
    }))
  }

  /// Cloneable handle the daemon's `TransportController` uses to send
  /// outbound HID commands when iAP2 holds playback authority. Returns
  /// `None` when the iAP2 manager isn't running (no MFi chip available).
  pub fn iap2_transport_handle(&self) -> Option<iap2::Iap2TransportHandle> {
    self.iap2_transport.clone()
  }

  /// Stock-webapp-driven "connect to this paired device" entry point.
  /// For iOS peers this kicks the accessory-initiated iAP2 dial into
  /// the iPhone's iAP2-device channel. Android peers can't be dialed
  /// from accessory side - the companion app is the initiator there
  /// - so the call is a no-op aside from the trace.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayType {
  Ble,
  Rfcomm,
  Iap2Ea,
}

/// Inbound message from a gateway to the daemon. `protocol` records
/// which transport carried the message — useful for tracing and for
/// any handler that needs to know the source transport. Outbound
/// responses do not carry a transport tag; they fan out across all
/// gateways and the gateway holding the peer connection delivers.
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

/// Outbound message from the daemon to a gateway-connected peer.
/// `address: Some(_)` targets one peer (delivered by whichever gateway
/// holds the connection); `address: None` broadcasts to every peer
/// across every gateway. No transport selection at the call site —
/// per the iAP2 strategy doc each peer is on at most one transport,
/// so fanning out across all gateways is safe and the right answer.
#[derive(Debug, Clone)]
pub struct OutboundGatewayMessage {
  pub address: Option<Address>,
  pub priority: Priority,
  pub msg: BridgeToGatewayMsg,
}

impl OutboundGatewayMessage {
  pub fn new(address: Option<Address>, msg: BridgeToGatewayMsg) -> Self {
    Self {
      address,
      priority: Priority::Normal,
      msg,
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

#[derive(Debug)]
pub struct GatewayMan {
  adapter: Adapter,
  state: State,

  tx: BluetoothTx,

  ble: Option<GatewayCon>,
  rfcomm: GatewayCon,
  iap2_ea_send_tx: GatewaySendTx,
  iap2_ea_handle: Iap2EaGatewayHandle,
  _iap2_ea_handle: JoinHandle<()>,
}

impl GatewayMan {
  pub async fn init(adapter: Adapter, session: &Session, state: State, tx: BluetoothTx) -> BluetoothResult<Self> {
    tracing::debug!("initializing bluetooth gateway manager");
    let ble = match GatewayCon::init(&adapter, session, state.clone(), GatewayType::Ble, tx.clone()).await {
      Ok(con) => Some(con),
      Err(err) => {
        tracing::warn!(
          "BLE gateway init failed; continuing without BLE (rfcomm + on-device webapp still work): {:?}",
          err
        );
        None
      }
    };
    let rfcomm = GatewayCon::init(&adapter, session, state.clone(), GatewayType::Rfcomm, tx.clone()).await?;

    let (iap2_ea, iap2_ea_handle) = Iap2EaGateway::init(state.clone(), tx.clone());
    let iap2_ea_send_tx = iap2_ea.send_tx();
    let _iap2_ea_handle = iap2_ea.spawn();

    Ok(Self {
      adapter,
      state,

      tx,

      ble,
      rfcomm,
      iap2_ea_send_tx,
      iap2_ea_handle,
      _iap2_ea_handle,
    })
  }

  pub fn iap2_ea_handle(&self) -> Iap2EaGatewayHandle {
    self.iap2_ea_handle.clone()
  }

  /// Fan an outbound message out to every gateway. Each gateway
  /// filters by `data.address` internally and no-ops silently if it
  /// has no connection for that peer; for `address: None` each
  /// gateway broadcasts to its own connected peers. At most one
  /// gateway has a given peer connected at a time, so the fan-out
  /// does not double-deliver.
  pub async fn send_all(&self, data: OutboundGatewayMessage) {
    if let Some(ble) = &self.ble {
      ble.send(data.clone()).await;
    }
    self.rfcomm.send(data.clone()).await;
    if let Err(err) = self.iap2_ea_send_tx.send(data).await {
      tracing::error!(?err, "failed to enqueue iap2 ea gateway send");
    }
  }

  pub async fn forward_all(&self, data: ForwardMessage) {
    let msg = BridgeToGatewayMsg {
      id: uuid::Uuid::now_v7(),
      meta: GatewayMsgMeta::Event,
      data: data.into(),
    };
    self.send_all(OutboundGatewayMessage::all(msg)).await;
  }
}

#[derive(Debug)]
pub struct GatewayCon {
  gateway_type: GatewayType,

  tx: GatewaySendTx,

  _handle: JoinHandle<()>,
  _listener: JoinHandle<()>,
}

impl GatewayCon {
  pub async fn init(
    adapter: &Adapter,
    session: &Session,
    state: State,
    gateway_type: GatewayType,
    bluetooth_tx: BluetoothTx,
  ) -> BluetoothResult<Self> {
    tracing::debug!("initializing bluetooth gateway connection handle for {gateway_type:?}");
    let (recv_tx, rx) = tokio::sync::mpsc::channel(16);
    let (tx, notify_rx) = tokio::sync::mpsc::channel(16);

    let _handle = match gateway_type {
      GatewayType::Ble => GattServer::init(adapter, state, recv_tx, notify_rx).await?.spawn(),
      GatewayType::Rfcomm => RfcommGateway::init(session, state, recv_tx, notify_rx).await?.spawn(),
      GatewayType::Iap2Ea => unreachable!("Iap2Ea is initialized via Iap2EaGateway, not GatewayCon"),
    };

    let _listener = Self::spawn_listener(gateway_type, rx, bluetooth_tx);

    Ok(Self {
      gateway_type,

      tx,

      _handle,
      _listener,
    })
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
  #[error("bluetooth gatt characteristic pipe broken!!")]
  CharacteristicControl,
}

crate::impl_broadcast_failure_from!(BluetoothError);
