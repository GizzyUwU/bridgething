use std::{sync::Arc, time::Duration};

use bluer::{agent::AgentHandle, Adapter, AdapterEvent, AdapterProperty, Address, Device};
use futures::{Stream, StreamExt};
use libbridgething::{server::ServerBluetoothEvent, ServerEventType};
use message::{connection_messages, disconnection_messages};
use tokio::sync::RwLock;

use crate::{
  player::PlayerError,
  state::{State, StateError},
  ws::WSError,
};

pub mod art;
mod auth;
#[cfg(debug_assertions)]
mod debug;
mod message;

pub type Bluetooth = Arc<BluetoothMan>;

pub type BluetoothTx = tokio::sync::mpsc::Sender<BluetoothEvent>;
pub type BluetoothRx = tokio::sync::mpsc::Receiver<BluetoothEvent>;

pub const AVRCP_UUID: bluer::Uuid = bluer::Uuid::from_u128(0x110c00001000800000805f9b34fb);

#[derive(Debug, Default)]
struct BluetoothState {
  pub device: Option<Device>,
}

pub struct BluetoothListener {
  rx: BluetoothRx,
  stream: Box<dyn Stream<Item = AdapterEvent> + Unpin>,
  _agent_handle: AgentHandle,
}

impl BluetoothListener {
  /// cancel-safe
  pub async fn recv(&mut self) -> BluetoothEvent {
    tokio::select! {
      Some(msg) = self.rx.recv() => {
        msg
      },
      Some(msg) = self.stream.next() => {
        msg.into()
      },
    }
  }
}

#[derive(Debug)]
pub struct BluetoothMan {
  state: State,

  pub adapter: Adapter,
  bt_state: RwLock<BluetoothState>,

  tx: BluetoothTx,
}

impl BluetoothMan {
  pub async fn init(state: State) -> Result<(Bluetooth, BluetoothListener), BluetoothError> {
    tracing::debug!("initializing bluetooth session");

    let session = bluer::Session::new().await?;

    let timeout = std::time::Duration::new(10, 0);
    let start = std::time::Instant::now();

    let adapter = loop {
      match session.default_adapter().await {
        Ok(adapter) => break adapter,
        Err(e) => {
          if start.elapsed() >= timeout {
            tracing::error!("Error getting default adapter - timing out: {:?}", e);
            return Err(BluetoothError::Timeout);
          }
          tracing::warn!("Error getting default adapter: {:?}", e);
          tokio::time::sleep(std::time::Duration::from_millis(750)).await;
          continue;
        }
      }
    };

    tracing::debug!("attempting to power on adapter");
    adapter.set_powered(true).await?;

    tracing::info!("initialized bluetooth adapter {}", adapter.name());

    tracing::debug!("configuring adapter");
    adapter.set_discoverable_timeout(0).await?;
    adapter.set_pairable_timeout(0).await?;
    adapter.set_pairable(true).await?;

    let (tx, rx) = tokio::sync::mpsc::channel(16);
    let _agent_handle = auth::build_agent(&session, tx.clone()).await?;

    #[cfg(debug_assertions)]
    debug::query_adapter(&adapter).await?;

    // start stream BEFORE device reconnection attempts
    let listener = BluetoothListener {
      rx,
      stream: Box::new(adapter.events().await?),
      _agent_handle,
    };

    // restore connections if possible
    if let Some(last) = state.last_device().await {
      if let Ok(mac) = last.parse() {
        tokio::spawn(connect_device(adapter.clone(), mac, tx.clone(), None));
      };
    }

    let this = Self {
      state,

      tx,
      bt_state: RwLock::new(BluetoothState::default()),

      adapter,
    };

    Ok((Arc::new(this), listener))
  }

  pub async fn set_alias(&self, alias: String) -> bluer::Result<()> {
    tracing::debug!("setting bluetooth adapter alias to {:?}", &alias);
    self.adapter.set_alias(alias).await
  }

  pub async fn set_discoverable(&self, discoverable: bool) -> bluer::Result<()> {
    tracing::debug!("setting bluetooth discoverable to {:?}", &discoverable);
    self.adapter.set_discoverable(discoverable).await
  }

  pub fn connect(&self, mac: &str) -> bluer::Result<()> {
    tracing::debug!("attempting to connect to device with mac address {:?}", &mac);
    tokio::spawn(connect_device(
      self.adapter.clone(),
      mac.parse()?,
      self.tx.clone(),
      Some(12),
    ));

    Ok(())
  }

  pub async fn forget(&self, mac: &str) -> bluer::Result<()> {
    tracing::debug!("attempting to forget device with mac address {:?}", &mac);

    let device = self.adapter.device(mac.parse()?)?;
    device.set_trusted(false).await?;
    device.disconnect().await?;

    Ok(())
  }

  pub async fn reset(&self) -> bluer::Result<()> {
    tracing::debug!("forgetting all devices");
    for mac in self.state.get_devices().await.keys() {
      self.forget(mac).await?;
    }

    Ok(())
  }

  pub async fn handle_event(&self, event: BluetoothEvent) -> Result<(), BluetoothError> {
    match event {
      // auth/pairing
      BluetoothEvent::AuthRequest { mac } => {
        tracing::info!("bluetooth auth request from mac address: {:?}", &mac);
        Ok(())
      }
      BluetoothEvent::ServiceAuthRequest { mac, service } => {
        tracing::info!(
          "bluetooth service auth request from mac address {:?} to service: {:?}",
          &mac,
          &service
        );
        Ok(())
      }
      BluetoothEvent::PinCode { mac, pin } => {
        tracing::info!(
          "bluetooth device with mac address {:?} pairing pincode: {:?}",
          &mac,
          &pin
        );

        self
          .state
          .client_man
          .broadcast(
            ServerBluetoothEvent::Pin {
              mac: mac.to_string(),
              name: mac.to_string(),
              pin: pin.to_owned(),
            },
            ServerEventType::Info,
          )
          .await?;

        Ok(())
      }

      // adapter
      BluetoothEvent::DeviceAdded { mac } => {
        tracing::info!("bluetooth device added with mac address: {:?}", &mac);
        let just_connected = self.handle_device(mac).await?;

        if let Some(device) = &self.bt_state.read().await.device {
          self.state.player.init_dbus_player(device.clone()).await?;

          if just_connected {
            tracing::info!("bluetooth device connected with mac address: {:?}", &mac);
            self.state.set_last_device(mac.to_string()).await?;

            let state_device = libbridgething::Device {
              name: device.name().await?.unwrap_or(mac.to_string()),
              device_type: libbridgething::DeviceType::Unknown,
              mac: mac.to_string(),
              default: true,
            };

            let mut new_device = false;
            if self.state.get_device(&mac.to_string()).await.is_none() {
              new_device = true;
              self.state.add_device(state_device.clone()).await?;

              self.set_discoverable(false).await?;
            };

            self.handle_connection(new_device).await?;
          };
        };

        Ok(())
      }
      BluetoothEvent::DeviceRemoved { mac } => {
        tracing::info!("bluetooth device removed with mac address: {:?}", &mac);

        if self
          .bt_state
          .write()
          .await
          .device
          .take_if(|d| d.address() == mac)
          .is_some()
        {
          tracing::info!("current device with mac address {:?} has disconnected!", &mac);
          self.state.handle_disconnect().await?;

          disconnection_messages(&self.state).await?;

          tracing::debug!("spawning reconnect loop for mac {:?}", &mac);
          #[cfg(not(debug_assertions))]
          tokio::spawn(connect_device(self.adapter.clone(), mac, self.tx.clone(), None));
        }

        Ok(())
      }
      BluetoothEvent::AdapterPropertyChanged(property) => {
        tracing::trace!("adapter property changed: {:?}", &property);
        Ok(())
      }
    }
  }

  pub async fn handle_connection(&self, new_device: bool) -> Result<(), BluetoothError> {
    let Some(device) = &self.bt_state.read().await.device else {
      return Ok(());
    };

    let Some(state_device) = &self.state.get_device(&device.address().to_string()).await else {
      return Ok(());
    };

    connection_messages(&self.state, new_device, state_device).await
  }

  /// the returned bool is whether this is a new pairing or not
  async fn handle_device(&self, mac: Address) -> bluer::Result<bool> {
    tracing::debug!("setting current bluetooth device to {:?}", &mac);
    let device = self.adapter.device(mac)?;

    #[cfg(debug_assertions)]
    debug::query_device(&device).await?;

    let state_device = &mut self.bt_state.write().await.device;
    if state_device.is_none() && device.is_paired().await? {
      if !device.is_trusted().await? {
        device.set_trusted(true).await?;
      }

      *state_device = Some(device);
      return Ok(true);
    }

    Ok(false)
  }
}

pub async fn connect_device(adapter: Adapter, mac: Address, tx: BluetoothTx, max_attempts: Option<usize>) {
  let mut attempts: usize = 0;
  let connected_device: Device;

  loop {
    if let Some(max) = max_attempts {
      if attempts > max {
        tracing::warn!("max connect attempts for mac {:?} exceeded.", &mac);
        return;
      }
    }

    tracing::debug!("attempting to connect to device with mac: {:?}", &mac);
    if let Ok(device) = adapter.device(mac) {
      tracing::debug!("found handle to device with mac: {:?}", &mac);

      if let Ok(connected) = device.is_connected().await {
        if connected {
          tracing::info!("connected to device with mac: {:?}", &mac);
          connected_device = device;
          break;
        } else if device.connect().await.is_ok() {
          tracing::info!("connected to device with mac: {:?}", &mac);
          connected_device = device;
          break;
        }
      };
    };

    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    attempts += 1;
  }

  connect_avrcp(&connected_device).await;

  if let Err(err) = tx.send(BluetoothEvent::DeviceAdded { mac }).await {
    tracing::error!("failed to send message to bluetooth tx: {:?}", err);
  }
}

pub async fn connect_avrcp(device: &Device) -> bool {
  loop {
    tracing::debug!("attempting to connect to avrcp profile...");
    match device.connect_profile(&AVRCP_UUID).await {
      Ok(()) => {
        tracing::info!("avrcp profile connected!");
        return true;
      }
      Err(err) => {
        tracing::debug!("failed to connect to avrcp profile: {:?}", err);
        tokio::time::sleep(Duration::from_secs(2)).await;
      }
    };
  }
}

#[derive(Debug)]
pub enum BluetoothEvent {
  // auth/pairing
  AuthRequest { mac: Address },
  ServiceAuthRequest { mac: Address, service: uuid::Uuid },
  PinCode { mac: Address, pin: String },

  // adapter
  DeviceAdded { mac: Address },
  DeviceRemoved { mac: Address },
  AdapterPropertyChanged(AdapterProperty),
}

impl From<AdapterEvent> for BluetoothEvent {
  fn from(event: AdapterEvent) -> Self {
    match event {
      AdapterEvent::DeviceAdded(address) => Self::DeviceAdded { mac: address },
      AdapterEvent::DeviceRemoved(address) => Self::DeviceRemoved { mac: address },
      AdapterEvent::PropertyChanged(property) => Self::AdapterPropertyChanged(property),
    }
  }
}

pub type BluetoothResult<T> = Result<T, BluetoothError>;
#[derive(Debug, thiserror::Error)]
pub enum BluetoothError {
  #[error("bluez error: {0}")]
  Bluez(#[from] bluer::Error),
  #[error("websocket error: {0}")]
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
