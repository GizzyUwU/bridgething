use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::msg::Device;

pub mod meta;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
  #[serde(skip)]
  path: PathBuf,
  #[serde(skip)]
  pub connected_device: Option<bluer::Address>,
  pub meta: meta::Meta,

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
    let mut state = if path.exists() && path.is_file() {
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

  pub async fn add_device(&mut self, device: Device) -> Result<(), StateError> {
    self.devices.insert(device.mac.clone(), device);
    self.save().await?;

    Ok(())
  }

  pub async fn remove_device(&mut self, mac: String) -> Result<(), StateError> {
    if self.devices.remove(&mac).is_some() {
      self.save().await?;
    }

    Ok(())
  }

  pub fn get_storage_key(&self, key: &str) -> Option<String> {
    let value = self.storage.get(key);
    value.cloned()
  }

  pub async fn set_storage_key(&mut self, key: String, value: String) -> Result<(), StateError> {
    self.storage.insert(key, value);
    self.save().await?;

    Ok(())
  }

  pub async fn del_storage_key(&mut self, key: &str) -> Result<(), StateError> {
    self.storage.remove(key);
    self.save().await?;

    Ok(())
  }

  async fn read(path: &PathBuf) -> Result<Self, StateError> {
    let data = tokio::fs::read(path).await?;
    let state: State = bincode::deserialize(&data)?;

    Ok(state)
  }

  // #[cfg(not(debug_assertions))]
  async fn save(&self) -> Result<(), StateError> {
    let data = bincode::serialize(&self)?;
    tokio::fs::write(&self.path, data).await?;

    Ok(())
  }

  // #[cfg(debug_assertions)]
  // async fn save(&self) -> Result<(), StateError> {
  //   tracing::trace!("debug mode: not saving application state.");
  //   Ok(())
  // }

  pub async fn reset(&self) -> Result<(), StateError> {
    Ok(tokio::fs::remove_file(&self.path).await?)
  }
}

#[derive(Debug, thiserror::Error)]
pub enum StateError {
  #[error("failed to bind to port: {0}")]
  Io(#[from] tokio::io::Error),
  #[error("bincode deserialization error: {0}")]
  Deserialize(#[from] bincode::Error),
}
