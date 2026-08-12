use std::path::Path;

use bridgething_companion::backend::{ModelArtifactKind, ModelArtifactValidator, ModelValidationError};

use crate::backends::asr;
#[cfg(target_os = "macos")]
use crate::backends::macos::nlu;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use crate::backends::nlu;

pub struct DesktopArtifactValidator;

impl ModelArtifactValidator for DesktopArtifactValidator {
  fn validate(&self, kind: ModelArtifactKind, path: String) -> Result<(), ModelValidationError> {
    let path = Path::new(&path);
    match kind {
      ModelArtifactKind::NluModel => nlu::check(path),
      ModelArtifactKind::AsrModel => asr::check(path),
    }
    .map_err(ModelValidationError::Invalid)
  }
}
