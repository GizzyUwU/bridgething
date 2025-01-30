use std::time::Duration;

use bluer::{agent::AgentHandle, Adapter, AdapterEvent, AdapterProperty, Address, Device};
use futures::{Stream, StreamExt};
use libbridgething::{server::ServerBluetoothEvent, ServerEventType};
use message::{connection_messages, disconnection_messages};

use crate::{
  dbus::{DBusError, Player},
  state::{
    art::{CoverArtCache, ImageCache},
    State, StateError,
  },
  ws::{ClientMan, WSError},
};

pub mod art;
mod auth;
#[cfg(debug_assertions)]
mod debug;
mod message;

pub type BluetoothTx = tokio::sync::mpsc::Sender<BluetoothEvent>;
pub type BluetoothRx = tokio::sync::mpsc::Receiver<BluetoothEvent>;

pub const AVRCP_UUID: bluer::Uuid = bluer::Uuid::from_u128(0x110C00001000800000805F9B34FB);

pub struct Bluetooth {
  client_man: ClientMan,
  cover_art_cache: CoverArtCache,

  tx: BluetoothTx,
  rx: BluetoothRx,
  stream: Box<dyn Stream<Item = AdapterEvent> + Unpin>,

  device: Option<Device>,

  pub adapter: Adapter,
  _agent_handle: AgentHandle,
}

impl Bluetooth {
  pub async fn init(client_man: ClientMan, state: &mut State) -> Result<Self, BluetoothError> {
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
    let this = Self {
      client_man,
      cover_art_cache: CoverArtCache::new(ImageCache::new()),

      tx,
      rx,
      stream: Box::new(adapter.events().await?),

      device: None,

      adapter,
      _agent_handle,
    };

    // restore connections if possible
    if let Some(last) = &state.last_device {
      if let Ok(mac) = last.parse() {
        tokio::spawn(connect_device(this.adapter.clone(), mac, this.tx.clone(), None));
      };
    }

    Ok(this)
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

  pub async fn reset(&self, state: &mut State) -> bluer::Result<()> {
    tracing::debug!("forgetting all devices");
    for mac in state.get_devices().keys() {
      self.forget(mac).await?;
    }

    Ok(())
  }

  /// cancel-safe
  pub async fn listen(&mut self) -> BluetoothEvent {
    tokio::select! {
      Some(msg) = self.rx.recv() => {
        msg
      },
      Some(msg) = self.stream.next() => {
        msg.into()
      },
    }
  }

  pub async fn handle_event(&mut self, state: &mut State, event: BluetoothEvent) -> Result<(), BluetoothError> {
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

        if let Some(device) = &self.device {
          match Player::init(self.client_man.clone(), self.cover_art_cache.clone(), device.clone()).await {
            Ok(player) => state.player = Some(player),
            Err(err) => tracing::error!("error connecting to player via dbus: {:?}", err),
          };

          if just_connected {
            tracing::info!("bluetooth device connected with mac address: {:?}", &mac);
            state.connected_device = Some(mac);
            state.last_device = Some(mac.to_string());

            let state_device = libbridgething::Device {
              name: device.name().await?.unwrap_or(mac.to_string()),
              device_type: libbridgething::DeviceType::Unknown,
              mac: mac.to_string(),
              default: true,
            };

            let mut new_device = false;
            if state.get_device(&mac.to_string()).is_none() {
              new_device = true;
              state.add_device(state_device.clone()).await?;

              self.set_discoverable(false).await?;
            };

            self.handle_connection(&self.client_man, state, new_device).await?;
          };
        };

        Ok(())
      }
      BluetoothEvent::DeviceRemoved { mac } => {
        tracing::info!("bluetooth device removed with mac address: {:?}", &mac);

        if self.device.take_if(|d| d.address() == mac).is_some() {
          tracing::info!("current device with mac address {:?} has disconnected!", &mac);
          state.connected_device = None;
          state.player = None;

          disconnection_messages(&self.client_man, state).await?;

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

  pub async fn handle_connection(
    &self,
    conn_man: &ClientMan,
    state: &State,
    new_device: bool,
  ) -> Result<(), BluetoothError> {
    let Some(device) = &self.device else {
      return Ok(());
    };

    let Some(state_device) = state.get_device(&device.address().to_string()) else {
      return Ok(());
    };

    connection_messages(conn_man, state, new_device, state_device).await
  }

  /// the returned bool is whether this is a new pairing or not
  async fn handle_device(&mut self, mac: Address) -> bluer::Result<bool> {
    tracing::debug!("setting current bluetooth device to {:?}", &mac);
    let device = self.adapter.device(mac)?;

    #[cfg(debug_assertions)]
    debug::query_device(&device).await?;

    if self.device.is_none() && device.is_paired().await? {
      if !device.is_trusted().await? {
        device.set_trusted(true).await?;
      }

      self.device = Some(device);
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
  DBus(#[from] DBusError),
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
