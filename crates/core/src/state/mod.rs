use std::{collections::HashMap, sync::Arc};

use libbridgething::{Device, GatewayInfo, WebappManifest};
use sea_orm::{
  ColumnTrait, DatabaseConnection, DbErr, EntityTrait, IntoActiveModel, QueryFilter, Set, TransactionTrait,
};
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::{
  asset::{AssetCache, AssetError},
  authority::AuthorityRegistry,
  capabilities::CapabilitiesRegistry,
  chrome,
  net::ClientMan,
  paths,
  peer::PeerTracker,
  transfer::{ChunkedTransfer, TransferError},
};

pub mod meta;
pub mod routes;
pub mod storage;
mod webapps;

pub use routes::RouteTable;
use storage::{
  device::{Column as DeviceColumn, Entity as DeviceEntity, Model as DeviceModel},
  kv_storage::{Column as KvColumn, Entity as KvEntity},
  meta::{Column as MetaColumn, Entity as MetaEntity, KEY_ACTIVE_WEBAPP, KEY_LAST_DEVICE},
};
pub use webapps::{InstallError, WebappRegistry};

pub type State = Arc<AppState>;

#[derive(Debug)]
pub struct AppState {
  pub client_man: ClientMan,
  pub meta: meta::SuperbirdMeta,
  pub player: crate::player::Player,
  pub chrome: chrome::Chrome,
  pub webapps: WebappRegistry,
  pub assets: AssetCache,
  pub transfers: ChunkedTransfer,
  pub authority: AuthorityRegistry,
  pub capabilities: CapabilitiesRegistry,
  pub peers: PeerTracker,
  pub ws_routes: RouteTable,
  pub stream_routes: RouteTable,

  db: DatabaseConnection,
  _asset_cache_handle: JoinHandle<()>,
  _transfer_handle: JoinHandle<()>,
}

impl AppState {
  pub async fn init(
    client_man: ClientMan,
    meta: meta::SuperbirdMeta,
    player: crate::player::Player,
    chrome: chrome::Chrome,
    authority: AuthorityRegistry,
    capabilities: CapabilitiesRegistry,
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

    let asset_pending = AssetCache::init(db.clone(), paths::assets_blobs_dir()).await?;
    let (assets, _asset_cache_handle) = asset_pending.spawn();

    let transfer_pending = ChunkedTransfer::init(paths::transfers_dir()).await?;
    let (transfers, _transfer_handle) = transfer_pending.spawn();

    let ws_routes = RouteTable::new();
    let stream_routes = RouteTable::new();
    let peers = PeerTracker::new(
      client_man.clone(),
      player.clone(),
      capabilities.clone(),
      ws_routes.clone(),
      stream_routes.clone(),
    );

    Ok(Arc::new(Self {
      client_man,
      meta,
      player,
      chrome,
      webapps,
      assets,
      transfers,
      authority,
      capabilities,
      peers,
      ws_routes,
      stream_routes,

      db,
      _asset_cache_handle,
      _transfer_handle,
    }))
  }

  pub async fn active_webapp(&self) -> StateResult<Option<Uuid>> {
    let stored = read_meta(&self.db, KEY_ACTIVE_WEBAPP).await?;
    let parsed = stored.as_deref().and_then(|s| Uuid::parse_str(s).ok());
    if let Some(id) = parsed
      && self.webapps.resolve(id).await.is_some()
    {
      return Ok(Some(id));
    }
    Ok(self.webapps.default_id().await)
  }

  pub async fn set_active_webapp(&self, id: Uuid) -> StateResult<()> {
    write_meta(&self.db, KEY_ACTIVE_WEBAPP, &id.simple().to_string()).await?;
    Ok(())
  }

  pub async fn gateway_info(&self) -> Option<GatewayInfo> {
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

  pub async fn data_get(&self, app_id: Uuid, key: &str) -> StateResult<Option<String>> {
    self.get_storage_key(&data_namespace_key(app_id, key)).await
  }

  pub async fn data_set(&self, app_id: Uuid, key: &str, value: String) -> StateResult<()> {
    self.set_storage_key(data_namespace_key(app_id, key), value).await
  }

  pub async fn data_delete(&self, app_id: Uuid, key: &str) -> StateResult<()> {
    self.del_storage_key(&data_namespace_key(app_id, key)).await
  }

  pub async fn config_get(&self, app_id: Uuid, key: &str) -> StateResult<Option<String>> {
    self.get_storage_key(&config_namespace_key(app_id, key)).await
  }

  pub async fn config_set(&self, app_id: Uuid, key: &str, value: String) -> StateResult<()> {
    self.set_storage_key(config_namespace_key(app_id, key), value).await
  }

  pub async fn config_delete(&self, app_id: Uuid, key: &str) -> StateResult<()> {
    self.del_storage_key(&config_namespace_key(app_id, key)).await
  }

  pub async fn config_list(&self, app_id: Uuid) -> StateResult<Vec<(String, String)>> {
    let prefix = config_namespace_prefix(app_id);
    let pattern = format!("{prefix}%");
    let rows = KvEntity::find()
      .filter(KvColumn::Key.like(&pattern))
      .all(&self.db)
      .await?;
    Ok(
      rows
        .into_iter()
        .filter_map(|m| m.key.strip_prefix(&prefix).map(|k| (k.to_string(), m.value)))
        .collect(),
    )
  }

  pub async fn seed_config_defaults(&self, manifest: &WebappManifest) -> StateResult<()> {
    for field in &manifest.config {
      let Some(default) = field.default_as_storage() else {
        continue;
      };
      let key = field.key();
      if self.config_get(manifest.id, key).await?.is_some() {
        continue;
      }
      self.config_set(manifest.id, key, default).await?;
    }
    Ok(())
  }

  pub async fn webapp_storage_purge(&self, app_id: Uuid) -> StateResult<()> {
    let data_pattern = format!("{}:data:%", app_id.simple());
    let config_pattern = format!("{}:config:%", app_id.simple());
    let tx = self.db.begin().await?;
    KvEntity::delete_many()
      .filter(KvColumn::Key.like(&data_pattern))
      .exec(&tx)
      .await?;
    KvEntity::delete_many()
      .filter(KvColumn::Key.like(&config_pattern))
      .exec(&tx)
      .await?;
    tx.commit().await?;
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
  let stored = read_meta(db, KEY_ACTIVE_WEBAPP).await?;
  let parsed = stored.as_deref().and_then(|s| Uuid::parse_str(s).ok());
  if let Some(id) = parsed
    && webapps.resolve(id).await.is_some()
  {
    return Ok(());
  }
  match webapps.default_id().await {
    Some(id) => {
      tracing::warn!(
        "persisted active webapp ({:?}) does not resolve; falling back to {}",
        stored,
        id
      );
      write_meta(db, KEY_ACTIVE_WEBAPP, &id.simple().to_string()).await?;
    }
    None => {
      tracing::warn!("no webapps installed and no builtin available; clearing active webapp meta");
      MetaEntity::delete_by_id(KEY_ACTIVE_WEBAPP.to_string()).exec(db).await?;
    }
  }
  Ok(())
}

fn data_namespace_key(app_id: Uuid, key: &str) -> String {
  format!("{}:data:{key}", app_id.simple())
}

fn config_namespace_key(app_id: Uuid, key: &str) -> String {
  format!("{}:config:{key}", app_id.simple())
}

fn config_namespace_prefix(app_id: Uuid) -> String {
  format!("{}:config:", app_id.simple())
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
  #[error(transparent)]
  Transfer(#[from] TransferError),
}
