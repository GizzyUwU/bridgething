use std::{path::Path, sync::Arc};

use bridgething_delivery::seam;

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ModelArtifactKind {
  NluModel,
  AsrModel,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum ModelValidationError {
  #[error("{0}")]
  Invalid(String),
}

#[uniffi::export(with_foreign)]
pub trait ModelArtifactValidator: Send + Sync {
  fn validate(&self, kind: ModelArtifactKind, path: String) -> Result<(), ModelValidationError>;
}

#[uniffi::export(with_foreign)]
pub trait TransferPolicy: Send + Sync {
  fn allows_large_transfer(&self) -> bool;
}

pub struct ForeignModelValidator(Arc<dyn ModelArtifactValidator>);

impl ForeignModelValidator {
  pub fn new(inner: Arc<dyn ModelArtifactValidator>) -> Self {
    Self(inner)
  }
}

impl seam::ArtifactValidator for ForeignModelValidator {
  fn validate(&self, kind: seam::ArtifactKind, staged: &Path) -> Result<(), String> {
    let kind = match kind {
      seam::ArtifactKind::NluModel => ModelArtifactKind::NluModel,
      seam::ArtifactKind::AsrModel => ModelArtifactKind::AsrModel,
    };
    if kind == ModelArtifactKind::NluModel {
      let decoder = nlu::NluDecoder::load(staged).map_err(|error| error.to_string())?;
      crate::voice::inference::check_catalog(&decoder.info()).map_err(|error| error.to_string())?;
    }
    self
      .0
      .validate(kind, staged.display().to_string())
      .map_err(|error| error.to_string())
  }
}

pub struct ForeignTransferPolicy(Arc<dyn TransferPolicy>);

impl ForeignTransferPolicy {
  pub fn new(inner: Arc<dyn TransferPolicy>) -> Self {
    Self(inner)
  }
}

impl seam::TransferPolicy for ForeignTransferPolicy {
  fn allows_large_transfer(&self) -> bool {
    self.0.allows_large_transfer()
  }
}

pub struct AlwaysAllows;

impl seam::TransferPolicy for AlwaysAllows {
  fn allows_large_transfer(&self) -> bool {
    true
  }
}

#[cfg(test)]
mod tests {
  use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
  };

  use bridgething_delivery::seam::ArtifactValidator;

  use super::*;

  struct AlwaysPasses(AtomicUsize);

  impl ModelArtifactValidator for AlwaysPasses {
    fn validate(&self, _kind: ModelArtifactKind, _path: String) -> Result<(), ModelValidationError> {
      self.0.fetch_add(1, Ordering::SeqCst);
      Ok(())
    }
  }

  #[test]
  fn a_staged_nlu_bundle_that_does_not_parse_whole_is_rejected_before_the_platform_sees_it() {
    let staged = tempfile::tempdir().expect("a scratch directory");
    std::fs::write(staged.path().join("model.tflite"), b"tflite-shaped bytes").expect("wrote the model file");

    let platform = Arc::new(AlwaysPasses(AtomicUsize::new(0)));
    let validator = ForeignModelValidator::new(platform.clone());

    let verdict = validator.validate(seam::ArtifactKind::NluModel, staged.path());
    assert!(
      verdict.is_err(),
      "a bundle with no manifest, tokenizer or decode tables cannot rotate in"
    );
    assert_eq!(
      platform.0.load(Ordering::SeqCst),
      0,
      "the core-side parse ran first, so the platform interpreter was never asked"
    );
  }

  #[test]
  fn an_asr_artifact_still_goes_straight_to_the_platform_validator() {
    let staged = tempfile::tempdir().expect("a scratch directory");
    let weights = staged.path().join("weights.bin");
    std::fs::write(&weights, b"ggml").expect("wrote the weights file");

    let platform = Arc::new(AlwaysPasses(AtomicUsize::new(0)));
    let validator = ForeignModelValidator::new(platform.clone());

    validator
      .validate(seam::ArtifactKind::AsrModel, &weights)
      .expect("the platform validator's word stands alone for asr");
    assert_eq!(platform.0.load(Ordering::SeqCst), 1);
  }
}
