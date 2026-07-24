use std::{path::PathBuf, sync::Arc};

use libbridgething::BridgeThingMeta;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use super::{KvStore, StateResult};

const BRIDGETHING_VERSION: &str = env!("CARGO_PKG_VERSION");
const BRIDGETHING_APP_NAME: &str = env!("CARGO_PKG_NAME");
const NICKNAME_KV_KEY: &str = "nickname";

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SuperbirdMeta {
  pub name: String,
  pub version: String,
  pub description: String,
  pub bt_mac: String,
  pub serial_number: String,
  pub fcc_id: String,
  pub ic_id: String,
  pub model_name: String,
  pub channel: String,
  pub image_variant: String,
  pub image_version: String,
  pub image_build_id: String,
  pub image_build_date: String,
  pub image_distro: String,
  pub image_machine: String,
}

impl SuperbirdMeta {
  pub async fn read_or_default() -> Self {
    #[cfg(debug_assertions)]
    let meta_path = PathBuf::from("./resources/superbird.json");
    #[cfg(not(debug_assertions))]
    let meta_path = PathBuf::from("/etc/superbird");

    if !meta_path.exists() {
      tracing::warn!(
        path = %meta_path.display(),
        "superbird metadata missing; bridgething is only officially supported on bridgethingOS"
      );
      return Self::default();
    }

    let data = match tokio::fs::read(&meta_path).await {
      Ok(data) => data,
      Err(err) => {
        tracing::warn!(path = %meta_path.display(), %err, "could not read superbird metadata; falling back to defaults");
        return Self::default();
      }
    };

    match serde_json::from_slice(&data) {
      Ok(meta) => meta,
      Err(err) => {
        tracing::warn!(path = %meta_path.display(), %err, "could not parse superbird metadata; struct shape may have drifted from the on-disk template");
        Self::default()
      }
    }
  }
}

#[derive(Debug, Clone)]
pub struct DeviceMeta {
  inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
  static_meta: SuperbirdMeta,
  kv: KvStore,
  nickname_tx: watch::Sender<Option<String>>,
}

impl DeviceMeta {
  pub async fn init(static_meta: SuperbirdMeta, kv: KvStore) -> Self {
    let initial = kv.device_get(NICKNAME_KV_KEY).await.unwrap_or_else(|err| {
      tracing::warn!(?err, "kv device_get nickname at startup failed; starting empty");
      None
    });
    let (nickname_tx, _rx) = watch::channel(initial);
    Self {
      inner: Arc::new(Inner {
        static_meta,
        kv,
        nickname_tx,
      }),
    }
  }

  pub fn static_meta(&self) -> &SuperbirdMeta {
    &self.inner.static_meta
  }

  pub fn nickname(&self) -> Option<String> {
    self.inner.nickname_tx.borrow().clone()
  }

  pub fn subscribe(&self) -> watch::Receiver<Option<String>> {
    self.inner.nickname_tx.subscribe()
  }

  pub async fn set_nickname(&self, next: Option<String>) -> StateResult<()> {
    match &next {
      Some(value) => self.inner.kv.device_set(NICKNAME_KV_KEY, value.clone()).await?,
      None => self.inner.kv.device_delete(NICKNAME_KV_KEY).await?,
    }
    self.inner.nickname_tx.send_replace(next);
    Ok(())
  }

  pub fn snapshot(&self) -> BridgeThingMeta {
    build_meta(&self.inner.static_meta, self.nickname())
  }
}

fn build_meta(meta: &SuperbirdMeta, nickname: Option<String>) -> BridgeThingMeta {
  BridgeThingMeta {
    bridgething_version: format!("v{}", BRIDGETHING_VERSION),
    libbridgething_version: BridgeThingMeta::libbridgething_version(),
    app_name: BRIDGETHING_APP_NAME.to_string(),
    nickname,
    app_version: BRIDGETHING_VERSION.to_string(),
    os_name: meta.name.clone(),
    os_version: meta.version.clone(),
    os_description: meta.description.clone(),
    bt_mac: meta.bt_mac.clone(),
    serial_number: meta.serial_number.clone(),
    fcc_id: meta.fcc_id.clone(),
    ic_id: meta.ic_id.clone(),
    model_name: meta.model_name.clone(),
    channel: meta.channel.clone(),
    image_variant: meta.image_variant.clone(),
    image_version: meta.image_version.clone(),
    image_build_id: meta.image_build_id.clone(),
    image_build_date: meta.image_build_date.clone(),
    image_distro: meta.image_distro.clone(),
    image_machine: meta.image_machine.clone(),
    discord: "https://tl.mt/d".to_string(),
    credits: "Joey Eamigh".to_string(),
  }
}
