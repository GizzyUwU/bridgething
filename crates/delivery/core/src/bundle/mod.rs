pub mod fetch;
mod store;

use serde::{Deserialize, Serialize};
pub use store::{BundleConfig, BundleState, BundleStore};

use crate::seam::ArtifactKind;

pub const ASR_MODEL_NAME: &str = "model.bin";

const NLU_ARCHIVE_NAME: &str = "bundle.zip";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactDigest {
  pub size: u64,
  pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleKind {
  Nlu,
  Asr,
}

impl BundleKind {
  pub fn slug(&self) -> &'static str {
    match self {
      BundleKind::Nlu => "nlu",
      BundleKind::Asr => "asr",
    }
  }

  pub fn artifact_kind(&self) -> ArtifactKind {
    match self {
      BundleKind::Nlu => ArtifactKind::NluModel,
      BundleKind::Asr => ArtifactKind::AsrModel,
    }
  }

  pub(crate) fn download_name(&self) -> &'static str {
    match self {
      BundleKind::Nlu => NLU_ARCHIVE_NAME,
      BundleKind::Asr => ASR_MODEL_NAME,
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundlePlatform {
  Ios,
  Android,
  Macos,
  Linux,
  Windows,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleArtifact {
  pub url: String,
  pub size: u64,
  pub sha256: String,
}

impl BundleArtifact {
  pub fn digest(&self) -> ArtifactDigest {
    ArtifactDigest {
      size: self.size,
      sha256: self.sha256.clone(),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleManifest {
  pub version: String,
  pub updated_at: String,
  pub ios: Option<BundleArtifact>,
  pub android: Option<BundleArtifact>,
  pub macos: Option<BundleArtifact>,
  pub linux: Option<BundleArtifact>,
  pub windows: Option<BundleArtifact>,
}

impl BundleManifest {
  pub fn artifact_for(&self, platform: BundlePlatform) -> Option<&BundleArtifact> {
    match platform {
      BundlePlatform::Ios => self.ios.as_ref(),
      BundlePlatform::Android => self.android.as_ref(),
      BundlePlatform::Macos => self.macos.as_ref(),
      BundlePlatform::Linux => self.linux.as_ref(),
      BundlePlatform::Windows => self.windows.as_ref(),
    }
  }
}
