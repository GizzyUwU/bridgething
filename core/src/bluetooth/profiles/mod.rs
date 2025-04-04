use std::sync::Arc;

use bluer::{Adapter, AdapterEvent, AdapterProperty, Address, Device};
use libbridgething::{ServerEventType, server::ServerBluetoothEvent};
use message::{connection_messages, disconnection_messages};
use tokio::sync::RwLock;

use crate::state::State;

use super::{BluetoothResult, BluetoothTx};

pub mod avrcp;
mod message;

pub type ProfileMan = Arc<ProfileManager>;

#[derive(Debug, Default)]
struct ProfileConnectionState {
  pub device: Option<Device>,
}

#[derive(Debug)]
pub struct ProfileManager {
  adapter: Adapter,
  state: State,
  tx: BluetoothTx,

  profile_state: RwLock<ProfileConnectionState>,
}

impl ProfileManager {
  pub async fn init(adapter: Adapter, state: State, tx: BluetoothTx) -> ProfileManager {
    tracing::debug!("initializing bluetooth profile connection manager");

    Self {
      adapter,
      state,
      tx,

      profile_state: RwLock::new(ProfileConnectionState::default()),
    }
  }

  pub async fn set_alias(&self, alias: String) -> bluer::Result<()> {
    tracing::debug!("setting bluetooth adapter alias to {:?}", &alias);
    self.adapter.set_alias(alias).await
  }

  pub async fn set_discoverable(&self, discoverable: bool) -> bluer::Result<()> {
    tracing::debug!("setting bluetooth discoverable to {:?}", &discoverable);
    self.adapter.set_discoverable(discoverable).await
  }

  pub fn connect(self: &ProfileMan, mac: &str) -> bluer::Result<()> {
    tracing::debug!("attempting to connect to device with mac address {:?}", &mac);
    tokio::spawn(connect_profiles(self.clone(), mac.parse()?, Some(12)));

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

  pub async fn handle_event(self: &ProfileMan, event: BluetoothConnectionEvent) -> BluetoothResult<()> {
    match event {
      // auth/pairing
      BluetoothConnectionEvent::AuthRequest { mac } => {
        tracing::info!("bluetooth auth request from mac address: {:?}", &mac);
        Ok(())
      }
      BluetoothConnectionEvent::ServiceAuthRequest { mac, service } => {
        tracing::info!(
          "bluetooth service auth request from mac address {:?} to service: {:?}",
          &mac,
          &service
        );
        Ok(())
      }
      BluetoothConnectionEvent::PinCode { mac, pin } => {
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
      BluetoothConnectionEvent::DeviceAdded { mac } => {
        tracing::info!("bluetooth device added with mac address: {:?}", &mac);
        let just_connected = self.handle_device(mac).await?;

        if let Some(device) = &self.profile_state.read().await.device {
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
      BluetoothConnectionEvent::DeviceRemoved { mac } => {
        tracing::info!("bluetooth device removed with mac address: {:?}", &mac);

        if self
          .profile_state
          .write()
          .await
          .device
          .take_if(|d| d.address() == mac)
          .is_some()
        {
          tracing::info!("current device with mac address {:?} has disconnected!", &mac);
          self.state.handle_disconnect().await?;

          disconnection_messages(&self.state).await?;

          // TODO: figure out a solution for this
          // tracing::debug!("spawning reconnect loop for mac {:?}", &mac);
          // #[cfg(not(debug_assertions))]
          // tokio::spawn(connect_profiles(self.clone(), mac, None));
        }

        Ok(())
      }
      BluetoothConnectionEvent::AdapterPropertyChanged(property) => {
        tracing::trace!("adapter property changed: {:?}", &property);
        Ok(())
      }
    }
  }

  pub async fn handle_connection(&self, new_device: bool) -> BluetoothResult<()> {
    let Some(device) = &self.profile_state.read().await.device else {
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
    super::debug::query_device(&device).await?;

    let state_device = &mut self.profile_state.write().await.device;
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

pub async fn connect_profiles(profile_man: ProfileMan, mac: Address, max_attempts: Option<usize>) {
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
    if let Ok(device) = profile_man.adapter.device(mac) {
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

  avrcp::connect_avrcp(&connected_device).await;

  if let Err(err) = profile_man
    .handle_event(BluetoothConnectionEvent::DeviceAdded { mac })
    .await
  {
    tracing::error!("failed to handle device added event: {:?}", err);
  }
}

#[derive(Debug)]
pub enum BluetoothConnectionEvent {
  // auth/pairing
  AuthRequest { mac: Address },
  ServiceAuthRequest { mac: Address, service: uuid::Uuid },
  PinCode { mac: Address, pin: String },

  // adapter
  DeviceAdded { mac: Address },
  DeviceRemoved { mac: Address },
  AdapterPropertyChanged(AdapterProperty),
}

impl From<AdapterEvent> for BluetoothConnectionEvent {
  fn from(event: AdapterEvent) -> Self {
    match event {
      AdapterEvent::DeviceAdded(address) => Self::DeviceAdded { mac: address },
      AdapterEvent::DeviceRemoved(address) => Self::DeviceRemoved { mac: address },
      AdapterEvent::PropertyChanged(property) => Self::AdapterPropertyChanged(property),
    }
  }
}
