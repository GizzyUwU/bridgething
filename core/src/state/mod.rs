use std::{collections::HashMap, path::PathBuf, sync::Arc};

use libbridgething::Device;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::{chrome, server::ClientMan};

mod files;
pub mod meta;

pub type State = Arc<AppState>;

#[derive(Debug, Default, Serialize, Deserialize)]
struct PersistentAppState {
  // TODO: only say that device is "connected" if it is connected to avrcp profile
  pub last_device: Option<String>,
  pub devices: HashMap<String, Device>,
  pub storage: HashMap<String, String>,
}

impl PersistentAppState {
  pub async fn restore_or_default(path: &PathBuf) -> Self {
    if path.exists() && path.is_file() && !cfg!(feature = "no-persist") {
      if let Ok(persist_state) = AppState::read_persist(path).await {
        persist_state
      } else {
        tracing::warn!("state file is corrupt!! this is probably not good.");
        PersistentAppState::default()
      }
    } else {
      tracing::debug!("no saved state - initializing default state");
      PersistentAppState::default()
    }
  }
}

#[derive(Debug)]
pub struct AppState {
  pub client_man: ClientMan,
  pub meta: meta::SuperbirdMeta,
  pub player: crate::player::Player,
  pub chrome: chrome::Chrome,
  pub fs: files::FileSystem,

  persist_path: PathBuf,
  persist: RwLock<PersistentAppState>,
}

impl AppState {
  pub async fn init(
    client_man: ClientMan,
    meta: meta::SuperbirdMeta,
    player: crate::player::Player,
    chrome: chrome::Chrome,
  ) -> Result<State, StateError> {
    tracing::info!("initializing state");
    let config_dir_path = dirs::config_dir()
      .unwrap_or("/home/superbird/.config".into())
      .join("bridgething");

    if !config_dir_path.exists() {
      tokio::fs::create_dir_all(&config_dir_path).await?;
    }

    let persist_path = config_dir_path.join("bridgething.db");
    let persist = PersistentAppState::restore_or_default(&persist_path).await;

    let fs = files::FileSystem::init().await?;
    tracing::debug!("file system initialized");

    Ok(Arc::new(Self {
      client_man,
      meta,
      player,
      chrome,
      fs,

      persist_path,
      persist: RwLock::new(persist),
    }))
  }

  pub async fn get_devices(&self) -> HashMap<String, Device> {
    // cloning here so that the lock is not held open
    self.persist.read().await.devices.clone()
  }

  pub async fn get_device(&self, mac: &str) -> Option<Device> {
    // cloning here so that the lock is not held open
    self.persist.read().await.devices.get(mac).cloned()
  }

  pub async fn add_device(&self, device: Device) -> StateResult<()> {
    self.persist.write().await.devices.insert(device.mac.clone(), device);
    self.save_persist().await?;

    Ok(())
  }

  pub async fn remove_device(&self, mac: String) -> StateResult<()> {
    let mut app = self.persist.write().await;
    if app.devices.remove(&mac).is_some() {
      self.save_persist().await?;
    }

    let mut persist = self.persist.write().await;
    if persist.last_device == Some(mac) {
      persist.last_device = None;
    }

    // if let Some(current) = &self.connected_device {
    //   if current.to_string() == mac {
    //     self.connected_device = None;
    //   }
    // }

    Ok(())
  }

  pub async fn handle_disconnect(&self) -> StateResult<()> {
    // TODO: handle player delete

    Ok(())
  }

  pub async fn last_device(&self) -> Option<String> {
    // cloning here so that the lock is not held open
    self.persist.read().await.last_device.clone()
  }

  pub async fn set_last_device(&self, mac: String) -> StateResult<()> {
    self.persist.write().await.last_device = Some(mac);
    self.save_persist().await?;

    Ok(())
  }

  pub async fn get_storage_key(&self, key: &str) -> Option<String> {
    // cloning here so that the lock is not held open
    self.persist.read().await.storage.get(key).cloned()
  }

  pub async fn set_storage_key(&self, key: String, value: String) -> StateResult<()> {
    self.persist.write().await.storage.insert(key, value);
    self.save_persist().await?;

    Ok(())
  }

  pub async fn del_storage_key(&self, key: &str) -> StateResult<()> {
    self.persist.write().await.storage.remove(key);
    self.save_persist().await?;

    Ok(())
  }

  async fn read_persist(path: &PathBuf) -> StateResult<PersistentAppState> {
    let data = tokio::fs::read(path).await?;
    let state: PersistentAppState = bincode::deserialize(&data)?;
    tracing::trace!("persisted state: {:?}", &state);

    Ok(state)
  }

  #[cfg(not(feature = "no-persist"))]
  async fn save_persist(&self) -> StateResult<()> {
    let data = bincode::serialize(&*self.persist.read().await)?;
    tokio::fs::write(&self.persist_path, data).await?;

    Ok(())
  }

  #[cfg(feature = "no-persist")]
  async fn save_persist(&self) -> StateResult<()> {
    tracing::trace!("debug mode: not saving application state.");
    Ok(())
  }

  pub async fn reset(&self) -> StateResult<()> {
    if self.persist_path.exists() {
      tokio::fs::remove_file(&self.persist_path).await?
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
  #[error("invalid path: {0}")]
  InvalidPath(String),
  #[error("file does not exist: {0}")]
  FileNotFound(String),
}
