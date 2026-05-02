use std::{collections::HashMap, sync::Arc};

use libbridgething::{Device, client::GatewayStatus};
use sea_orm::{DatabaseConnection, DbErr, EntityTrait, IntoActiveModel, Set, TransactionTrait};
use tokio::task::JoinHandle;

use crate::{
  asset::{AssetCache, AssetError},
  authority::AuthorityRegistry,
  chrome,
  net::ClientMan,
  paths,
  peer::PeerTracker,
};

pub mod meta;
pub mod storage;
mod webapps;

use storage::{
  device::{Column as DeviceColumn, Entity as DeviceEntity, Model as DeviceModel},
  kv_storage::{Column as KvColumn, Entity as KvEntity},
  meta::{Column as MetaColumn, Entity as MetaEntity, KEY_ACTIVE_WEBAPP, KEY_LAST_DEVICE},
};
pub use webapps::WebappRegistry;

pub type State = Arc<AppState>;

const DEFAULT_ACTIVE_WEBAPP: &str = "stock";

#[derive(Debug)]
pub struct AppState {
  pub client_man: ClientMan,
  pub meta: meta::SuperbirdMeta,
  pub player: crate::player::Player,
  pub chrome: chrome::Chrome,
  pub webapps: WebappRegistry,
  pub assets: AssetCache,
  pub authority: AuthorityRegistry,
  pub peers: PeerTracker,

  db: DatabaseConnection,
  _asset_cache_handle: JoinHandle<()>,
}

impl AppState {
  pub async fn init(
    client_man: ClientMan,
    meta: meta::SuperbirdMeta,
    player: crate::player::Player,
    chrome: chrome::Chrome,
    authority: AuthorityRegistry,
  ) -> Result<State, StateError> {
    tracing::info!("initializing state");
    let state_dir = paths::state_dir();

    if !state_dir.exists() {
      tokio::fs::create_dir_all(&state_dir).await?;
    }

    let db = open_state_db(&state_dir).await?;

    let webapps = WebappRegistry::init().await?;
    tracing::debug!("webapp registry initialized");

    enforce_active_webapp_exists(&db, &webapps).await?;

    let asset_pending = AssetCache::init(db.clone()).await?;
    let (assets, _asset_cache_handle) = asset_pending.spawn();

    let peers = PeerTracker::new(client_man.clone(), player.clone(), authority.clone());

    Ok(Arc::new(Self {
      client_man,
      meta,
      player,
      chrome,
      webapps,
      assets,
      authority,
      peers,

      db,
      _asset_cache_handle,
    }))
  }

  pub async fn active_webapp(&self) -> StateResult<String> {
    Ok(
      read_meta(&self.db, KEY_ACTIVE_WEBAPP)
        .await?
        .unwrap_or_else(|| DEFAULT_ACTIVE_WEBAPP.to_string()),
    )
  }

  pub async fn set_active_webapp(&self, name: String) -> StateResult<()> {
    write_meta(&self.db, KEY_ACTIVE_WEBAPP, &name).await?;
    Ok(())
  }

  pub async fn gateway_status(&self) -> GatewayStatus {
    self.peers.first_connected_gateway().await
  }

  pub async fn get_devices(&self) -> StateResult<HashMap<String, Device>> {
    let rows = DeviceEntity::find().all(&self.db).await?;
    Ok(rows.iter().map(|m| (m.mac.clone(), Device::from(m))).collect())
  }

  pub async fn get_device(&self, mac: &str) -> StateResult<Option<Device>> {
    Ok(
      DeviceEntity::find_by_id(mac.to_string())
        .one(&self.db)
        .await?
        .as_ref()
        .map(Device::from),
    )
  }

  pub async fn add_device(&self, device: Device) -> StateResult<()> {
    let model = DeviceModel::from_wire(&device).into_active_model();
    DeviceEntity::insert(model)
      .on_conflict(
        sea_orm::sea_query::OnConflict::column(DeviceColumn::Mac)
          .update_columns([DeviceColumn::Name, DeviceColumn::DeviceType, DeviceColumn::IsDefault])
          .to_owned(),
      )
      .exec(&self.db)
      .await?;
    Ok(())
  }

  pub async fn remove_device(&self, mac: String) -> StateResult<()> {
    let tx = self.db.begin().await?;
    DeviceEntity::delete_by_id(mac.clone()).exec(&tx).await?;
    let last = MetaEntity::find_by_id(KEY_LAST_DEVICE.to_string()).one(&tx).await?;
    if last.map(|m| m.value) == Some(mac) {
      MetaEntity::delete_by_id(KEY_LAST_DEVICE.to_string()).exec(&tx).await?;
    }
    tx.commit().await?;
    Ok(())
  }

  pub async fn handle_disconnect(&self) -> StateResult<()> {
    Ok(())
  }

  pub async fn last_device(&self) -> StateResult<Option<String>> {
    read_meta(&self.db, KEY_LAST_DEVICE).await.map_err(Into::into)
  }

  pub async fn set_last_device(&self, mac: String) -> StateResult<()> {
    write_meta(&self.db, KEY_LAST_DEVICE, &mac).await?;
    Ok(())
  }

  pub async fn get_storage_key(&self, key: &str) -> StateResult<Option<String>> {
    Ok(
      KvEntity::find_by_id(key.to_string())
        .one(&self.db)
        .await?
        .map(|m| m.value),
    )
  }

  pub async fn set_storage_key(&self, key: String, value: String) -> StateResult<()> {
    let model = storage::kv_storage::ActiveModel {
      key: Set(key),
      value: Set(value),
    };
    KvEntity::insert(model)
      .on_conflict(
        sea_orm::sea_query::OnConflict::column(KvColumn::Key)
          .update_column(KvColumn::Value)
          .to_owned(),
      )
      .exec(&self.db)
      .await?;
    Ok(())
  }

  pub async fn del_storage_key(&self, key: &str) -> StateResult<()> {
    KvEntity::delete_by_id(key.to_string()).exec(&self.db).await?;
    Ok(())
  }

  pub async fn reset(&self) -> StateResult<()> {
    let tx = self.db.begin().await?;
    DeviceEntity::delete_many().exec(&tx).await?;
    KvEntity::delete_many().exec(&tx).await?;
    MetaEntity::delete_many().exec(&tx).await?;
    tx.commit().await?;
    Ok(())
  }
}

#[cfg(not(feature = "no-persist"))]
async fn open_state_db(state_dir: &std::path::Path) -> Result<DatabaseConnection, StateError> {
  let path = state_dir.join("bridgething.db");
  Ok(crate::db::open(Some(&path)).await?)
}

#[cfg(feature = "no-persist")]
async fn open_state_db(_state_dir: &std::path::Path) -> Result<DatabaseConnection, StateError> {
  tracing::trace!("debug mode: in-memory state database");
  Ok(crate::db::open(None).await?)
}

async fn enforce_active_webapp_exists(db: &DatabaseConnection, webapps: &WebappRegistry) -> Result<(), StateError> {
  let current = read_meta(db, KEY_ACTIVE_WEBAPP)
    .await?
    .unwrap_or_else(|| DEFAULT_ACTIVE_WEBAPP.to_string());
  if webapps.resolve(&current).is_some() {
    return Ok(());
  }
  tracing::warn!(
    "active webapp '{}' not present on disk; falling back to '{}'",
    current,
    DEFAULT_ACTIVE_WEBAPP
  );
  write_meta(db, KEY_ACTIVE_WEBAPP, DEFAULT_ACTIVE_WEBAPP).await?;
  Ok(())
}

async fn read_meta(db: &DatabaseConnection, key: &str) -> Result<Option<String>, DbErr> {
  Ok(MetaEntity::find_by_id(key.to_string()).one(db).await?.map(|m| m.value))
}

async fn write_meta(db: &DatabaseConnection, key: &str, value: &str) -> Result<(), DbErr> {
  let model = storage::meta::ActiveModel {
    key: Set(key.to_string()),
    value: Set(value.to_string()),
  };
  MetaEntity::insert(model)
    .on_conflict(
      sea_orm::sea_query::OnConflict::column(MetaColumn::Key)
        .update_column(MetaColumn::Value)
        .to_owned(),
    )
    .exec(db)
    .await?;
  Ok(())
}

pub type StateResult<T> = Result<T, StateError>;
#[derive(Debug, thiserror::Error)]
pub enum StateError {
  #[error("io error: {0}")]
  Io(#[from] tokio::io::Error),
  #[error("database error: {0}")]
  Db(#[from] DbErr),
  #[error("invalid path: {0}")]
  InvalidPath(String),
  #[error(transparent)]
  Asset(#[from] AssetError),
}
