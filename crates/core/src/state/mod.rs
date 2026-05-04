use std::sync::Arc;

use libbridgething::GatewayInfo;
use sea_orm::{DatabaseConnection, DbErr, EntityTrait, TransactionTrait};
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::{
  als::AlsManager,
  asset::{AssetCache, AssetError},
  authority::AuthorityRegistry,
  capabilities::CapabilitiesRegistry,
  chrome,
  mic::MicManager,
  net::{ClientMan, WireEventBus},
  paths,
  peer::PeerTracker,
  telephony::TelephonyManager,
  time::TimeManager,
  transfer::{ChunkedTransfer, TransferError},
};

pub mod meta;
pub mod routes;
pub mod storage;
mod webapps;

pub use routes::RouteTable;
pub use storage::{DeviceStore, KvStore, MetaStore};
use storage::{device::Entity as DeviceEntity, kv_storage::Entity as KvEntity, meta::Entity as MetaEntity};
pub use webapps::{InstallError, WebappRegistry};

pub type State = Arc<AppState>;

#[derive(Debug)]
pub struct AppState {
  pub client_man: ClientMan,
  pub bus: WireEventBus,
  pub meta: meta::SuperbirdMeta,
  pub player: crate::player::Player,
  pub chrome: chrome::Chrome,
  pub webapps: WebappRegistry,
  pub assets: AssetCache,
  pub transfers: ChunkedTransfer,
  pub authority: AuthorityRegistry,
  pub capabilities: CapabilitiesRegistry,
  pub peers: PeerTracker,
  pub telephony: TelephonyManager,
  pub time: TimeManager,
  pub als: AlsManager,
  pub mic: MicManager,
  pub devices: DeviceStore,
  pub kv: KvStore,
  pub ws_routes: RouteTable,
  pub stream_routes: RouteTable,

  db: DatabaseConnection,
  meta_store: MetaStore,
  _asset_cache_handle: JoinHandle<()>,
  _transfer_handle: JoinHandle<()>,
  _als_handle: JoinHandle<()>,
  _mic_handle: JoinHandle<()>,
}

#[allow(clippy::too_many_arguments)]
impl AppState {
  pub fn assemble(parts: AssembledState) -> State {
    let AssembledState {
      client_man,
      bus,
      meta,
      player,
      chrome,
      webapps,
      assets,
      transfers,
      authority,
      capabilities,
      peers,
      telephony,
      time,
      als,
      mic,
      devices,
      kv,
      ws_routes,
      stream_routes,
      db,
      meta_store,
      asset_cache_handle,
      transfer_handle,
      als_handle,
      mic_handle,
    } = parts;
    Arc::new(Self {
      client_man,
      bus,
      meta,
      player,
      chrome,
      webapps,
      assets,
      transfers,
      authority,
      capabilities,
      peers,
      telephony,
      time,
      als,
      mic,
      devices,
      kv,
      ws_routes,
      stream_routes,
      db,
      meta_store,
      _asset_cache_handle: asset_cache_handle,
      _transfer_handle: transfer_handle,
      _als_handle: als_handle,
      _mic_handle: mic_handle,
    })
  }

  pub async fn active_webapp(&self) -> StateResult<Option<Uuid>> {
    self.meta_store.active_webapp(&self.webapps).await
  }

  pub async fn set_active_webapp(&self, id: Uuid) -> StateResult<()> {
    self.meta_store.set_active_webapp(id).await
  }

  pub async fn gateway_info(&self) -> Option<GatewayInfo> {
    self.peers.first_connected_gateway().await
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

pub struct AssembledState {
  pub client_man: ClientMan,
  pub bus: WireEventBus,
  pub meta: meta::SuperbirdMeta,
  pub player: crate::player::Player,
  pub chrome: chrome::Chrome,
  pub webapps: WebappRegistry,
  pub assets: AssetCache,
  pub transfers: ChunkedTransfer,
  pub authority: AuthorityRegistry,
  pub capabilities: CapabilitiesRegistry,
  pub peers: PeerTracker,
  pub telephony: TelephonyManager,
  pub time: TimeManager,
  pub als: AlsManager,
  pub mic: MicManager,
  pub devices: DeviceStore,
  pub kv: KvStore,
  pub ws_routes: RouteTable,
  pub stream_routes: RouteTable,
  pub db: DatabaseConnection,
  pub meta_store: MetaStore,
  pub asset_cache_handle: JoinHandle<()>,
  pub transfer_handle: JoinHandle<()>,
  pub als_handle: JoinHandle<()>,
  pub mic_handle: JoinHandle<()>,
}

pub async fn open_state_db() -> StateResult<DatabaseConnection> {
  let state_dir = paths::state_dir();
  if !state_dir.exists() {
    tokio::fs::create_dir_all(&state_dir).await?;
  }
  open_db_from_dir(&state_dir).await
}

#[cfg(not(feature = "no-persist"))]
async fn open_db_from_dir(state_dir: &std::path::Path) -> Result<DatabaseConnection, StateError> {
  let path = state_dir.join("bridgething.db");
  Ok(crate::db::open(Some(&path)).await?)
}

#[cfg(feature = "no-persist")]
async fn open_db_from_dir(_state_dir: &std::path::Path) -> Result<DatabaseConnection, StateError> {
  tracing::trace!("debug mode: in-memory state database");
  Ok(crate::db::open(None).await?)
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
