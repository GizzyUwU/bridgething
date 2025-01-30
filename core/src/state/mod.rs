use std::{collections::HashMap, path::PathBuf};

use libbridgething::Device;
use serde::{Deserialize, Serialize};

use crate::dbus;

pub mod art;
pub mod meta;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
  #[serde(skip)]
  path: PathBuf,
  #[serde(skip)]
  pub connected_device: Option<bluer::Address>,
  #[serde(skip)]
  pub meta: meta::Meta,
  #[serde(skip)]
  pub player: Option<dbus::Player>,

  pub last_device: Option<String>,
  devices: HashMap<String, Device>,
  storage: HashMap<String, String>,
}

impl State {
  pub async fn init() -> Result<Self, StateError> {
    tracing::info!("initializing state");
    let config_dir_path = dirs::config_dir()
      .unwrap_or("/home/superbird/.config".into())
      .join("bridgething");

    if !config_dir_path.exists() {
      tokio::fs::create_dir_all(&config_dir_path).await?;
    }

    let path = config_dir_path.join("bridgething.db");
    let mut state = if path.exists() && path.is_file() && !cfg!(feature = "no-persist") {
      if let Ok(mut state) = State::read(&path).await {
        state.path = path;
        state
      } else {
        tracing::warn!("state file is corrupt!! this is probably not good.");
        Self {
          path,
          ..Default::default()
        }
      }
    } else {
      tracing::debug!("no saved state - initializing default state");
      Self {
        path,
        ..Default::default()
      }
    };

    #[cfg(debug_assertions)]
    let meta_path = PathBuf::from("./resources/superbird.json");
    #[cfg(not(debug_assertions))]
    let meta_path = PathBuf::from("/etc/superbird");

    if meta_path.exists() {
      let data = tokio::fs::read(&meta_path).await?;
      if let Ok(meta) = serde_json::from_slice(&data) {
        state.meta = meta;
      } else {
        tracing::warn!(
          "could not find superbird metadata! bridgething is only officially supported on nixos-superbird."
        );
      }
    } else {
      tracing::warn!("could not find superbird metadata! bridgething is only officially supported on nixos-superbird.");
    }

    tracing::debug!("metadata: {:?}", &state.meta);

    Ok(state)
  }

  pub fn get_devices(&self) -> &HashMap<String, Device> {
    &self.devices
  }

  pub fn get_device(&self, mac: &str) -> Option<&Device> {
    self.devices.get(mac)
  }

  pub async fn add_device(&mut self, device: Device) -> StateResult<()> {
    self.devices.insert(device.mac.clone(), device);
    self.save().await?;

    Ok(())
  }

  pub async fn remove_device(&mut self, mac: String) -> StateResult<()> {
    if self.devices.remove(&mac).is_some() {
      self.save().await?;
    }

    if let Some(last) = &self.last_device {
      if *last == mac {
        self.last_device = None;
      }
    }

    if let Some(current) = &self.connected_device {
      if current.to_string() == mac {
        self.connected_device = None;
      }
    }

    Ok(())
  }

  pub fn get_storage_key(&self, key: &str) -> Option<String> {
    let value = self.storage.get(key);
    value.cloned()
  }

  pub async fn set_storage_key(&mut self, key: String, value: String) -> StateResult<()> {
    self.storage.insert(key, value);
    self.save().await?;

    Ok(())
  }

  pub async fn del_storage_key(&mut self, key: &str) -> StateResult<()> {
    self.storage.remove(key);
    self.save().await?;

    Ok(())
  }

  async fn read(path: &PathBuf) -> StateResult<Self> {
    let data = tokio::fs::read(path).await?;
    let state: State = bincode::deserialize(&data)?;
    tracing::trace!("persisted state: {:?}", &state);

    Ok(state)
  }

  #[cfg(not(feature = "no-persist"))]
  async fn save(&self) -> StateResult<()> {
    let data = bincode::serialize(&self)?;
    tokio::fs::write(&self.path, data).await?;

    Ok(())
  }

  #[cfg(feature = "no-persist")]
  async fn save(&self) -> StateResult<()> {
    tracing::trace!("debug mode: not saving application state.");
    Ok(())
  }

  pub async fn reset(&self) -> StateResult<()> {
    if self.path.exists() {
      tokio::fs::remove_file(&self.path).await?
    }

    Ok(())
  }
}

pub type StateResult<T> = Result<T, StateError>;
#[derive(Debug, thiserror::Error)]
pub enum StateError {
  #[error("failed to bind to port: {0}")]
  Io(#[from] tokio::io::Error),
  #[error("bincode deserialization error: {0}")]
  Deserialize(#[from] bincode::Error),
}
