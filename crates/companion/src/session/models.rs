use std::{path::PathBuf, sync::Arc};

use bridgething_delivery::{
  bundle::{BundleConfig, BundleKind, BundlePlatform, BundleState, BundleStore, fetch::ArtifactFetch},
  seam::{ArtifactValidator, TransferPolicy},
};
use tokio::task::JoinHandle;

use crate::api::{VoiceModelState, VoiceModelStatus};

const ENV_MODEL_ROOT: &str = "BRIDGETHING_MODEL_ROOT";

pub struct VoiceModels {
  nlu: Arc<BundleStore>,
  asr: Option<Arc<BundleStore>>,
}

impl VoiceModels {
  pub fn new(
    platform: BundlePlatform,
    state_dir: PathBuf,
    enabled: bool,
    fetch: Arc<dyn ArtifactFetch>,
    policy: Arc<dyn TransferPolicy>,
    validator: Arc<dyn ArtifactValidator>,
  ) -> Arc<Self> {
    let root = std::env::var(ENV_MODEL_ROOT).ok();
    let store = |kind| {
      let mut config = BundleConfig::new(state_dir.clone(), platform);
      if let Some(root) = root.clone() {
        config.root_url = root;
      }
      Arc::new(BundleStore::new(
        kind,
        config,
        enabled,
        fetch.clone(),
        policy.clone(),
        validator.clone(),
      ))
    };
    Arc::new(Self {
      nlu: store(BundleKind::Nlu),
      asr: matches!(
        platform,
        BundlePlatform::Android | BundlePlatform::Macos | BundlePlatform::Linux | BundlePlatform::Windows
      )
      .then(|| store(BundleKind::Asr)),
    })
  }

  pub fn state(&self) -> VoiceModelState {
    let mut parts = vec![self.nlu.state()];
    if let Some(asr) = &self.asr {
      parts.push(asr.state());
    }
    merge(&parts)
  }

  pub fn asr_weights(&self) -> Option<PathBuf> {
    self.asr.as_ref().and_then(|store| store.live())
  }

  pub fn nlu_bundle(&self) -> Option<PathBuf> {
    self.nlu.live()
  }

  pub async fn ensure(&self) {
    self.nlu.ensure().await;
    if let Some(asr) = &self.asr {
      asr.ensure().await;
    }
  }

  pub async fn download_now(&self) {
    self.nlu.download_now().await;
    if let Some(asr) = &self.asr {
      asr.download_now().await;
    }
  }

  pub async fn set_enabled(&self, value: bool) {
    self.nlu.set_enabled(value).await;
    if let Some(asr) = &self.asr {
      asr.set_enabled(value).await;
    }
  }

  pub fn watch(self: &Arc<Self>, on_change: Arc<dyn Fn(VoiceModelState) + Send + Sync>) -> JoinHandle<()> {
    let models = self.clone();
    let mut nlu = self.nlu.subscribe();
    let mut asr = self.asr.as_ref().map(|store| store.subscribe());
    let mut last = self.state();
    let alive = |received: Result<BundleState, tokio::sync::broadcast::error::RecvError>| {
      !matches!(received, Err(tokio::sync::broadcast::error::RecvError::Closed))
    };
    tokio::spawn(async move {
      loop {
        let moved = match &mut asr {
          Some(asr) => tokio::select! {
            state = nlu.recv() => alive(state),
            state = asr.recv() => alive(state),
          },
          None => alive(nlu.recv().await),
        };
        if !moved {
          return;
        }
        let state = models.state();
        if state != last {
          last = state.clone();
          on_change(state);
        }
      }
    })
  }
}

fn merge(parts: &[BundleState]) -> VoiceModelState {
  let bare = |status| VoiceModelState {
    status,
    received_bytes: 0,
    total_bytes: 0,
    version: None,
    error: None,
  };

  let mut received = 0;
  let mut total = 0;
  let mut downloading = false;
  for part in parts {
    if let BundleState::Downloading {
      received: part_received,
      total: part_total,
    } = part
    {
      downloading = true;
      received += part_received;
      total += part_total;
    }
  }
  if downloading {
    return VoiceModelState {
      received_bytes: received,
      total_bytes: total,
      ..bare(VoiceModelStatus::Downloading)
    };
  }

  for part in parts {
    if let BundleState::Failed { reason } = part {
      return VoiceModelState {
        error: Some(reason.clone()),
        ..bare(VoiceModelStatus::Failed)
      };
    }
  }

  let versions: Vec<&str> = parts
    .iter()
    .filter_map(|part| match part {
      BundleState::Ready { version } => Some(version.as_str()),
      _ => None,
    })
    .collect();
  if versions.len() != parts.len() {
    return bare(VoiceModelStatus::Absent);
  }
  let mut distinct = versions.clone();
  distinct.sort_unstable();
  distinct.dedup();
  let version = if distinct.len() == 1 {
    distinct[0].to_owned()
  } else {
    distinct.join(" + ")
  };
  VoiceModelState {
    version: Some(version),
    ..bare(VoiceModelStatus::Ready)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn any_download_shows_as_one_combined_download() {
    let merged = merge(&[
      BundleState::Downloading {
        received: 10,
        total: 100,
      },
      BundleState::Ready {
        version: "1".to_owned(),
      },
    ]);
    assert_eq!(merged.status, VoiceModelStatus::Downloading);
    assert_eq!(merged.received_bytes, 10);
    assert_eq!(merged.total_bytes, 100);
  }

  #[test]
  fn a_failure_wins_over_absence() {
    let merged = merge(&[
      BundleState::Failed {
        reason: "no".to_owned(),
      },
      BundleState::Absent,
    ]);
    assert_eq!(merged.status, VoiceModelStatus::Failed);
    assert_eq!(merged.error.as_deref(), Some("no"));
  }

  #[test]
  fn ready_needs_every_store_and_joins_distinct_versions() {
    let ready = |version: &str| BundleState::Ready {
      version: version.to_owned(),
    };
    assert_eq!(
      merge(&[ready("1"), BundleState::Absent]).status,
      VoiceModelStatus::Absent
    );
    assert_eq!(merge(&[ready("1"), ready("1")]).version.as_deref(), Some("1"));
    assert_eq!(merge(&[ready("1"), ready("2")]).version.as_deref(), Some("1 + 2"));
  }
}
