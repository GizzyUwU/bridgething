// protocol modules
mod ble;
mod profiles;
mod rfcomm;

// general modules
mod adapter;
mod auth;
#[cfg(debug_assertions)]
mod debug;

// reexports // TODO: review these
pub use profiles::avrcp;
use rfcomm::RfcommGateway;

use std::sync::Arc;

use crate::{
  player::PlayerError,
  server::WSError,
  state::{State, StateError},
};
use ble::GattServer;
use bluer::{Adapter, Address, Session};
use libbridgething::{
  ForwardMessage,
  gateway::{BridgeToGatewayMsg, GatewayMsgMeta, GatewayToBridgeMsg},
};
use profiles::ProfileMan;
use tokio::task::JoinHandle;

pub type BluetoothMan = Arc<BluetoothManager>;
pub type BluetoothTx = tokio::sync::mpsc::Sender<BluetoothEvent>;

#[derive(Debug)]
pub enum BluetoothEvent {
  Gateway(GatewayMessage<GatewayToBridgeMsg>),
}

#[derive(Debug)]
pub struct BluetoothManager {
  session: Session,
  pub adapter: Adapter,
  state: State,
  tx: BluetoothTx,

  pub profile_man: ProfileMan,
  pub gateway_man: GatewayMan,
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

    // if we had a bdedr device with profile connections, try to reconnect
    if let Some(last) = state.last_device().await {
      if let Ok(mac) = last.parse() {
        tokio::spawn(profiles::connect_profiles(profile_man.clone(), mac, None));
      };
    }

    Ok(Arc::new(Self {
      session,
      adapter,
      state,

      tx,

      profile_man,
      gateway_man,
    }))
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayType {
  Ble,
  Rfcomm,
}

#[derive(Debug, Clone)]
pub struct GatewayMessage<T: Clone> {
  pub address: Option<Address>,
  pub protocol: GatewayType,
  pub msg: T,
}

impl<T: Clone> GatewayMessage<T> {
  pub fn new(address: Option<Address>, protocol: GatewayType, msg: T) -> Self {
    Self { address, protocol, msg }
  }

  pub fn all(protocol: GatewayType, msg: T) -> Self {
    Self::new(None, protocol, msg)
  }

  pub fn ble(address: Address, msg: T) -> Self {
    Self::new(Some(address), GatewayType::Ble, msg)
  }

  pub fn ble_all(msg: T) -> Self {
    Self::new(None, GatewayType::Ble, msg)
  }

  pub fn rfcomm(address: Address, msg: T) -> Self {
    Self::new(Some(address), GatewayType::Rfcomm, msg)
  }

  pub fn rfcomm_all(msg: T) -> Self {
    Self::new(None, GatewayType::Rfcomm, msg)
  }
}

pub type GatewayRecvTx = tokio::sync::mpsc::Sender<GatewayMessage<GatewayToBridgeMsg>>;
pub type GatewayRecvRx = tokio::sync::mpsc::Receiver<GatewayMessage<GatewayToBridgeMsg>>;
pub type GatewaySendTx = tokio::sync::mpsc::Sender<GatewayMessage<BridgeToGatewayMsg>>;
pub type GatewaySendRx = tokio::sync::mpsc::Receiver<GatewayMessage<BridgeToGatewayMsg>>;

#[derive(Debug)]
pub struct GatewayMan {
  adapter: Adapter,
  state: State,

  tx: BluetoothTx,

  ble: GatewayCon,
  rfcomm: GatewayCon,
}

impl GatewayMan {
  pub async fn init(adapter: Adapter, session: &Session, state: State, tx: BluetoothTx) -> BluetoothResult<Self> {
    tracing::debug!("initializing bluetooth gateway manager");
    let ble = GatewayCon::init(&adapter, session, state.clone(), GatewayType::Ble, tx.clone()).await?;
    let rfcomm = GatewayCon::init(&adapter, session, state.clone(), GatewayType::Rfcomm, tx.clone()).await?;

    Ok(Self {
      adapter,
      state,

      tx,

      ble,
      rfcomm,
    })
  }

  pub async fn send_all(&self, data: GatewayMessage<BridgeToGatewayMsg>) {
    match &data.protocol {
      GatewayType::Ble => self.ble.send(data).await,
      GatewayType::Rfcomm => self.rfcomm.send(data).await,
    }
  }

  pub async fn forward_all(&self, data: ForwardMessage) {
    let msg = BridgeToGatewayMsg {
      id: uuid::Uuid::now_v7(),
      meta: GatewayMsgMeta::Event,
      data: data.into(),
    };

    // if let Err(err) = self.ble.tx.send(GatewayMessage::ble_all(msg.clone())).await {
    //   tracing::error!("failed to send message to bluetooth gateway: {:?}", err);
    // }
    if let Err(err) = self.rfcomm.tx.send(GatewayMessage::rfcomm_all(msg)).await {
      tracing::error!("failed to send message to bluetooth gateway: {:?}", err);
    }
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
    };

    let _listener = Self::spawn_listener(gateway_type, rx, bluetooth_tx);

    Ok(Self {
      gateway_type,

      tx,

      _handle,
      _listener,
    })
  }

  pub async fn send(&self, data: GatewayMessage<BridgeToGatewayMsg>) {
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

impl From<Vec<WSError>> for BluetoothError {
  fn from(errors: Vec<WSError>) -> Self {
    for error in errors {
      tracing::error!("failed to broadcast message: {:?}", error);
    }

    Self::WS(WSError::BroadcastFailed)
  }
}
