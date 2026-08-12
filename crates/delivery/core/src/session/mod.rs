mod handlers;

use std::{sync::Arc, time::Duration};

use bridgething_gateway::{
  Gateway, GatewayProtocol,
  routing::{Routing, spawn_routing},
};
use bridgething_sdk_runtime::{Connector, rt};
pub use handlers::DeliveryHandlers;
use libbridgething::{
  GatewayCapabilities, GatewayInfo,
  gateway::{BridgeToGatewayMsg, GatewayToBridgeCapabilitiesMsgEvent},
};
use tokio::sync::broadcast;

use crate::{
  bundle::fetch::ArtifactFetch,
  ota::service::{OtaService, OtaServiceDeps},
  seam::Clock,
};

pub const ANNOUNCE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SessionError {
  #[error("announce failed: {0}")]
  Announce(String),
}

pub struct SessionDeps {
  pub device_id: String,
  pub clock: Arc<dyn Clock>,
  pub fetch: Arc<dyn ArtifactFetch>,
  pub cache_dir: std::path::PathBuf,
  pub info: GatewayInfo,
}

pub fn gateway_info(name: &str, os_name: &str, app_version: &str) -> GatewayInfo {
  GatewayInfo {
    address: String::new(),
    name: name.into(),
    os_name: os_name.into(),
    app_name: name.into(),
    app_version: app_version.into(),
    adapter_version: String::new(),
    lib_version: env!("CARGO_PKG_VERSION").to_string(),
    libbridgething_version: format!("v{}", libbridgething::LIBBRIDGETHING_VERSION),
  }
}

pub struct DeliverySession {
  pub gateway: Gateway,
  pub ota: Arc<OtaService>,
  pub handlers: Arc<DeliveryHandlers>,
  device_id: String,
  routing: Routing,
}

impl DeliverySession {
  pub async fn spawn<C: Connector<GatewayProtocol>>(connector: C, deps: SessionDeps) -> Result<Self, SessionError> {
    let (gateway, inbound) = Gateway::spawn_subscribed(connector);
    Self::adopt(gateway, inbound, deps).await
  }

  #[cfg(any(target_arch = "wasm32", feature = "native-ws"))]
  pub async fn connect(url: &str, deps: SessionDeps) -> Result<Self, SessionError> {
    let (gateway, inbound) = Gateway::connect_subscribed(url)
      .await
      .map_err(|e| SessionError::Announce(format!("ws connect {url}: {e:?}")))?;
    Self::adopt(gateway, inbound, deps).await
  }

  async fn adopt(
    gateway: Gateway,
    inbound: broadcast::Receiver<BridgeToGatewayMsg>,
    deps: SessionDeps,
  ) -> Result<Self, SessionError> {
    let ota = OtaService::new(OtaServiceDeps {
      clock: deps.clock.clone(),
      fetch: deps.fetch,
      cache_dir: deps.cache_dir,
    });
    ota.adopt(&deps.device_id, gateway.clone()).await;

    #[cfg(not(target_arch = "wasm32"))]
    let handlers = DeliveryHandlers::new(
      deps.device_id.clone(),
      ota.clone(),
      crate::serve::tunnel::TunnelDispatcher::new(Arc::new(gateway.clone()), deps.clock.clone()),
    );
    #[cfg(target_arch = "wasm32")]
    let handlers = DeliveryHandlers::new(deps.device_id.clone(), ota.clone());

    let notifier = handlers.clone();
    let mut announced = std::pin::pin!(notifier.announced.notified());
    announced.as_mut().enable();

    let routing = spawn_routing(gateway.clone(), handlers.clone(), inbound);

    announce(&gateway, deps.info).await?;
    if rt::timeout(ANNOUNCE_TIMEOUT, announced).await.is_err() {
      tracing::warn!("daemon did not announce a version; continuing anyway");
    }

    Ok(Self {
      gateway,
      ota,
      handlers,
      device_id: deps.device_id,
      routing,
    })
  }

  pub fn device_id(&self) -> &str {
    &self.device_id
  }

  pub async fn closed(&self) {
    self.routing.closed().await;
  }
}

async fn announce(gateway: &Gateway, info: GatewayInfo) -> Result<(), SessionError> {
  let caps = GatewayCapabilities {
    gateway: info,
    ..Default::default()
  };
  gateway
    .event(GatewayToBridgeCapabilitiesMsgEvent::Announce(caps))
    .await
    .map_err(|e| SessionError::Announce(e.to_string()))
}
