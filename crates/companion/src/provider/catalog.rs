use std::sync::Arc;

use bridgething_io::{HttpTransport as IoHttpTransport, WsTransport as IoWsTransport};

use crate::{
  api::{ProviderTokens, SpotifyProviderConfig},
  backend::{
    AppleMusicBackend, DeviceWaker, ForeignHttp, ForeignWs, HttpTransport, ImageScaler, SecretStore, WsTransport,
  },
  provider::{
    Provider,
    apple_music::{self, AppleMusicProvider},
    spotify::{self, SpotifyConfig, SpotifyProvider},
  },
};

pub trait CatalogEntry: Send + Sync {
  fn id(&self) -> &str;
  fn display_name(&self) -> &str;
  fn build(&self) -> Arc<dyn Provider>;
  fn has_credentials(&self) -> bool;
  fn clear_credentials(&self);
  fn adopt_tokens(&self, tokens: ProviderTokens);
  fn mark_connected(&self);
}

#[derive(Default)]
pub struct ProviderCatalog {
  entries: Vec<Arc<dyn CatalogEntry>>,
}

impl ProviderCatalog {
  pub fn new(entries: Vec<Arc<dyn CatalogEntry>>) -> Self {
    Self { entries }
  }

  pub fn entries(&self) -> &[Arc<dyn CatalogEntry>] {
    &self.entries
  }

  pub fn get(&self, id: &str) -> Option<Arc<dyn CatalogEntry>> {
    self.entries.iter().find(|entry| entry.id() == id).cloned()
  }
}

pub struct AppleMusicEntry {
  backend: Arc<dyn AppleMusicBackend>,
  http: Arc<dyn HttpTransport>,
  secrets: Arc<dyn SecretStore>,
  image: Option<Arc<dyn ImageScaler>>,
}

impl AppleMusicEntry {
  pub fn new(
    backend: Arc<dyn AppleMusicBackend>,
    http: Arc<dyn HttpTransport>,
    secrets: Arc<dyn SecretStore>,
    image: Option<Arc<dyn ImageScaler>>,
  ) -> Arc<Self> {
    Arc::new(Self {
      backend,
      http,
      secrets,
      image,
    })
  }
}

impl CatalogEntry for AppleMusicEntry {
  fn id(&self) -> &str {
    apple_music::PROVIDER_NAME
  }

  fn display_name(&self) -> &str {
    "Apple Music"
  }

  fn build(&self) -> Arc<dyn Provider> {
    AppleMusicProvider::new(
      self.backend.clone(),
      Arc::new(ForeignHttp::new(self.http.clone())) as Arc<dyn IoHttpTransport>,
      self.image.clone(),
    )
  }

  fn has_credentials(&self) -> bool {
    self.secrets.get(apple_music::KEY_CONNECTED.into()).is_some()
  }

  fn clear_credentials(&self) {
    self.secrets.remove(apple_music::KEY_CONNECTED.into());
  }

  fn adopt_tokens(&self, _tokens: ProviderTokens) {}

  fn mark_connected(&self) {
    self.secrets.set(apple_music::KEY_CONNECTED.into(), "1".into());
  }
}

pub struct SpotifyEntry {
  config: SpotifyProviderConfig,
  http: Arc<dyn HttpTransport>,
  ws: Arc<dyn WsTransport>,
  secrets: Arc<dyn SecretStore>,
  image: Option<Arc<dyn ImageScaler>>,
  waker: Option<Arc<dyn DeviceWaker>>,
}

impl SpotifyEntry {
  pub fn new(
    config: SpotifyProviderConfig,
    http: Arc<dyn HttpTransport>,
    ws: Arc<dyn WsTransport>,
    secrets: Arc<dyn SecretStore>,
    image: Option<Arc<dyn ImageScaler>>,
    waker: Option<Arc<dyn DeviceWaker>>,
  ) -> Arc<Self> {
    Arc::new(Self {
      config,
      http,
      ws,
      secrets,
      image,
      waker,
    })
  }

  fn device_id(&self) -> String {
    if let Some(id) = self.secrets.get(spotify::KEY_DEVICE_ID.into()) {
      return id;
    }
    let id = uuid::Uuid::now_v7().simple().to_string();
    self.secrets.set(spotify::KEY_DEVICE_ID.into(), id.clone());
    id
  }
}

impl CatalogEntry for SpotifyEntry {
  fn id(&self) -> &str {
    spotify::PROVIDER_NAME
  }

  fn display_name(&self) -> &str {
    "Spotify"
  }

  fn build(&self) -> Arc<dyn Provider> {
    SpotifyProvider::new(
      SpotifyConfig {
        worker_base: self.config.worker_base.clone(),
        psk: self.config.psk.clone(),
        device_id: self.device_id(),
      },
      Arc::new(ForeignHttp::new(self.http.clone())) as Arc<dyn IoHttpTransport>,
      Arc::new(ForeignWs::new(self.ws.clone())) as Arc<dyn IoWsTransport>,
      self.secrets.clone(),
      self.image.clone(),
      self.waker.clone(),
    )
  }

  fn has_credentials(&self) -> bool {
    self.secrets.get(spotify::KEY_REFRESH_TOKEN.into()).is_some()
  }

  fn clear_credentials(&self) {
    self.secrets.remove(spotify::KEY_REFRESH_TOKEN.into());
    self.secrets.remove(spotify::KEY_USERNAME.into());
  }

  fn adopt_tokens(&self, tokens: ProviderTokens) {
    self
      .secrets
      .set(spotify::KEY_REFRESH_TOKEN.into(), tokens.refresh_token);
  }

  fn mark_connected(&self) {}
}
