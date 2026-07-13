use std::sync::Arc;

use libbridgething::GatewayInfo;
use sea_orm::{DatabaseConnection, DbErr, EntityTrait, TransactionTrait};
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::{
  als::AlsManager,
  asset::{AssetCache, AssetError, wait::AssetWaitTracker},
  authority::AuthorityRegistry,
  capabilities::CapabilitiesRegistry,
  chrome,
  handler::{gateway::SpotifyWakeGate, iap2::Iap2PendingArt},
  mic::MicManager,
  net::{ClientMan, WireEventBus},
  paths,
  peer::PeerTracker,
  transfer::{TransferError, outbound::TransferOutbound, sinks::TransferSinks},
};

mod audio;
mod browse_content;
mod geo_watchers;
pub mod log_tap;
pub mod meta;
mod root_browse;
pub mod routes;
pub mod storage;
mod telephony;
mod time;
mod tunnel_routes;
mod webapps;

pub use audio::{AudioError, AudioManager};
pub use browse_content::BrowseContentCache;
pub use geo_watchers::{GeoWatchers, WatchAggregate, WatchChange};
pub use log_tap::{LogTap, LogTapLayer};
pub use root_browse::RootBrowseCache;
pub use routes::RouteTable;
pub use storage::{DeviceStore, KvStore, MetaStore};
use storage::{device::Entity as DeviceEntity, kv_storage::Entity as KvEntity, meta::Entity as MetaEntity};
pub use telephony::TelephonyManager;
pub use time::TimeManager;
pub use tunnel_routes::{TunnelInbound, TunnelRoutes};
pub use webapps::{BROWSER_WEBAPP_ID, HUB_WEBAPP_ID, STOCK_WEBAPP_ID, WebappRegistry};
pub(crate) use webapps::{extract_zip, sha256_hex};

pub type State = Arc<AppState>;

#[derive(Debug)]
pub struct AppState {
  pub client_man: ClientMan,
  pub bus: WireEventBus,
  pub meta: meta::DeviceMeta,
  pub player: crate::player::Player,
  pub chrome: chrome::Chrome,
  pub webapps: WebappRegistry,
  pub assets: AssetCache,
  pub asset_wait: AssetWaitTracker,
  pub iap2_pending_art: Iap2PendingArt,
  pub spotify_wake_gate: SpotifyWakeGate,
  pub authority: AuthorityRegistry,
  pub capabilities: CapabilitiesRegistry,
  pub peers: PeerTracker,
  pub telephony: TelephonyManager,
  pub time: TimeManager,
  pub audio: AudioManager,
  pub als: AlsManager,
  pub mic: MicManager,
  pub devices: DeviceStore,
  pub kv: KvStore,
  pub ws_routes: RouteTable,
  pub stream_routes: RouteTable,
  pub geo_watchers: GeoWatchers,
  pub log_tap: LogTap,
  pub tunnel_routes: TunnelRoutes,
  pub root_browse: RootBrowseCache,
  pub browse_content: BrowseContentCache,
  pub transfer_sinks: TransferSinks,
  pub transfer_outbound: TransferOutbound,

  db: DatabaseConnection,
  meta_store: MetaStore,
  _asset_cache_handle: JoinHandle<()>,
  _transfer_handle: JoinHandle<()>,
  _als_handle: JoinHandle<()>,
  _mic_handle: JoinHandle<()>,
}

impl AppState {
  pub fn assemble(parts: StateAssembly) -> State {
    let StateAssembly {
      client_man,
      bus,
      meta,
      player,
      chrome,
      webapps,
      assets,
      asset_wait,
      iap2_pending_art,
      spotify_wake_gate,
      authority,
      capabilities,
      peers,
      telephony,
      time,
      audio,
      als,
      mic,
      devices,
      kv,
      ws_routes,
      stream_routes,
      geo_watchers,
      log_tap,
      tunnel_routes,
      transfer_sinks,
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
      asset_wait,
      iap2_pending_art,
      spotify_wake_gate,
      authority,
      capabilities,
      peers,
      telephony,
      time,
      audio,
      als,
      mic,
      devices,
      kv,
      ws_routes,
      stream_routes,
      geo_watchers,
      log_tap,
      tunnel_routes,
      root_browse: RootBrowseCache::default(),
      browse_content: BrowseContentCache::default(),
      transfer_sinks,
      transfer_outbound: TransferOutbound::default(),
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

  pub async fn active_webapp_changed_event(&self) -> libbridgething::gateway::WebappActiveChanged {
    let id = self.active_webapp().await.ok().flatten();
    let (name, art) = match id {
      Some(id) => match self.webapps.bundle(id).await {
        Some(b) => (Some(b.manifest.name.clone()), b.manifest.art),
        None => (None, None),
      },
      None => (None, None),
    };
    libbridgething::gateway::WebappActiveChanged { id, name, art }
  }

  pub async fn set_active_webapp(&self, id: Uuid) -> StateResult<()> {
    let prev = self.active_webapp().await?;
    self.meta_store.set_active_webapp(id).await?;
    if prev != Some(id) {
      self.tunnel_routes.kill_all();
    }
    Ok(())
  }

  pub fn gateway_info(&self) -> Option<GatewayInfo> {
    self.peers.first_connected_gateway()
  }

  pub async fn reset(&self) -> StateResult<()> {
    let tx = self.db.begin().await?;
    DeviceEntity::delete_many().exec(&tx).await?;
    KvEntity::delete_many().exec(&tx).await?;
    MetaEntity::delete_many().exec(&tx).await?;
    tx.commit().await?;
    if let Err(err) = self.assets.clear_all().await {
      tracing::warn!(?err, "factory reset: asset cache wipe failed");
    }
    Ok(())
  }
}

pub struct StateAssembly {
  pub client_man: ClientMan,
  pub bus: WireEventBus,
  pub meta: meta::DeviceMeta,
  pub player: crate::player::Player,
  pub chrome: chrome::Chrome,
  pub webapps: WebappRegistry,
  pub assets: AssetCache,
  pub asset_wait: AssetWaitTracker,
  pub iap2_pending_art: Iap2PendingArt,
  pub spotify_wake_gate: SpotifyWakeGate,
  pub authority: AuthorityRegistry,
  pub capabilities: CapabilitiesRegistry,
  pub peers: PeerTracker,
  pub telephony: TelephonyManager,
  pub time: TimeManager,
  pub audio: AudioManager,
  pub als: AlsManager,
  pub mic: MicManager,
  pub devices: DeviceStore,
  pub kv: KvStore,
  pub ws_routes: RouteTable,
  pub stream_routes: RouteTable,
  pub geo_watchers: GeoWatchers,
  pub log_tap: LogTap,
  pub tunnel_routes: TunnelRoutes,
  pub transfer_sinks: TransferSinks,
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
  #[error(transparent)]
  Asset(#[from] AssetError),
  #[error(transparent)]
  Transfer(#[from] TransferError),
}
