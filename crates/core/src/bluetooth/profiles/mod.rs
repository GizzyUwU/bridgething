use std::sync::Arc;

use bluer::{Adapter, AdapterEvent, AdapterProperty, Address, Device};
use libbridgething::{ServerEventType, server::ServerBluetoothEvent};
use message::{connection_messages_stock, disconnection_messages_stock};
use tokio::sync::RwLock;

use crate::state::State;

use super::{BluetoothResult, BluetoothTx};

pub mod avrcp;
mod message;

pub type ProfileMan = Arc<ProfileManager>;

// TODO: only say that device is "connected" if it is connected to avrcp profile
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

  #[expect(clippy::manual_async_fn)]
  pub fn handle_event(
    self: &ProfileMan,
    event: BluetoothConnectionEvent,
  ) -> impl Future<Output = BluetoothResult<()>> + Send {
    async {
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
              ServerEventType::Event,
            )
            .await?;

          Ok(())
        }

        // adapter
        BluetoothConnectionEvent::DeviceAdded { mac } => {
          tracing::info!("bluetooth device added with mac address: {:?}", &mac);
          let bluez_device = self.adapter.device(mac)?;
          if !bluez_device.is_paired().await.unwrap_or(false) {
            tracing::trace!("device added but not yet paired; awaiting pair-complete event");
            return Ok(());
          }
          if let Err(err) = self
            .upsert_paired_device(mac, libbridgething::DeviceType::Unknown)
            .await
          {
            tracing::warn!(?err, "failed to register newly-paired device");
          }
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

            let _ = self.state.peers.remove(mac).await;
            disconnection_messages_stock(&self.state).await?;

            tracing::debug!("spawning reconnect loop for mac {:?}", &mac);
            #[cfg(not(debug_assertions))]
            tokio::spawn(connect_profiles(self.clone(), mac, None));
          }

          Ok(())
        }
        BluetoothConnectionEvent::AdapterPropertyChanged(property) => {
          tracing::trace!("adapter property changed: {:?}", &property);
          Ok(())
        }
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

    connection_messages_stock(&self.state, new_device, state_device).await
  }

  pub async fn upsert_paired_device(
    &self,
    mac: Address,
    device_type: libbridgething::DeviceType,
  ) -> BluetoothResult<libbridgething::Device> {
    let bluez = self.adapter.device(mac)?;
    if !bluez.is_trusted().await.unwrap_or(false) {
      let _ = bluez.set_trusted(true).await;
    }
    let name = bluez.name().await?.unwrap_or_else(|| mac.to_string());
    let mac_str = mac.to_string();

    let device = libbridgething::Device {
      name,
      device_type,
      mac: mac_str.clone(),
      default: true,
    };

    let already_active = self.state.peers.get(&mac).await.is_some_and(|p| p.paired);
    let new_device = self.state.get_device(&mac_str).await.is_none();
    if new_device {
      self.state.add_device(device.clone()).await?;
      self.set_discoverable(false).await?;
    }
    self.state.set_last_device(mac_str).await?;

    {
      let mut profile_state = self.profile_state.write().await;
      profile_state.device = Some(bluez);
    }

    let _ = self.state.peers.upsert(mac, device.clone()).await;
    let _ = self.state.peers.set_paired(mac, true).await;

    if !already_active {
      connection_messages_stock(&self.state, new_device, &device).await?;
    }

    Ok(device)
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
