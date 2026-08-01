use std::{
  io,
  path::{Path, PathBuf},
};

use tokio::fs;
use uuid::Uuid;

use super::staging;

#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
  #[error("no writable model location is configured")]
  NoTarget,
  #[error("io error during {step}: {source}")]
  Io {
    step: &'static str,
    #[source]
    source: io::Error,
  },
  #[error("rejected: {0}")]
  Unusable(String),
}

fn io_err(step: &'static str) -> impl Fn(io::Error) -> ApplyError {
  move |source| ApplyError::Io { step, source }
}

pub fn target() -> Option<PathBuf> {
  crate::paths::wakeword_models().into_iter().next()
}

pub async fn apply(payload: &Path, dest: &Path) -> Result<(), ApplyError> {
  let dir = dest.parent().ok_or(ApplyError::NoTarget)?.to_path_buf();
  fs::create_dir_all(&dir).await.map_err(io_err("mkdir model dir"))?;

  let incoming = dir.join(format!(".incoming.{}", Uuid::now_v7().simple()));
  if let Err(err) = fs::copy(payload, &incoming).await {
    staging::remove_any(&incoming).await;
    return Err(ApplyError::Io {
      step: "copy payload",
      source: err,
    });
  }

  if let Err(err) = verify(incoming.clone()).await {
    staging::remove_any(&incoming).await;
    return Err(err);
  }

  let previous = dest.with_extension("btww.previous");
  if fs::try_exists(dest).await.unwrap_or(false) {
    staging::remove_any(&previous).await;
    fs::rename(dest, &previous).await.map_err(io_err("rotate current"))?;
  }
  if let Err(err) = fs::rename(&incoming, dest).await {
    if fs::try_exists(&previous).await.unwrap_or(false) {
      let _ = fs::rename(&previous, dest).await;
    }
    staging::remove_any(&incoming).await;
    return Err(ApplyError::Io {
      step: "promote incoming",
      source: err,
    });
  }

  tracing::info!(model = %dest.display(), "wake word model applied");
  Ok(())
}

#[cfg(feature = "mic")]
async fn verify(path: PathBuf) -> Result<(), ApplyError> {
  tokio::task::spawn_blocking(move || bridgething_wakeword::classifier::Classifier::load(&path))
    .await
    .map_err(|err| ApplyError::Unusable(format!("verify task panicked: {err}")))?
    .map_err(|err| ApplyError::Unusable(format!("{err}")))?;
  Ok(())
}

#[cfg(not(feature = "mic"))]
async fn verify(_path: PathBuf) -> Result<(), ApplyError> {
  Err(ApplyError::Unusable(
    "this build has no wake word runtime to validate a model against".into(),
  ))
}

pub async fn sweep_orphans() {
  let Some(dest) = target() else { return };
  let Some(dir) = dest.parent() else { return };
  let Ok(mut rd) = fs::read_dir(dir).await else {
    return;
  };
  while let Ok(Some(entry)) = rd.next_entry().await {
    if entry.file_name().to_string_lossy().starts_with(".incoming.") {
      staging::remove_any(&entry.path()).await;
    }
  }
}

#[cfg(all(test, feature = "mic"))]
mod tests {
  use super::*;

  fn temp_dir() -> PathBuf {
    let p = std::env::temp_dir().join(format!("bridgething-wakeword-swap-{}", Uuid::now_v7().simple()));
    std::fs::create_dir_all(&p).unwrap();
    p
  }

  fn real_model() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../wakeword/models/hey_bridgething.btww")
  }

  #[tokio::test]
  async fn a_real_model_is_promoted_and_the_old_one_is_kept() {
    let root = temp_dir();
    let dest = root.join("hey_bridgething.btww");
    fs::write(&dest, b"the model that was live").await.unwrap();

    apply(&real_model(), &dest).await.expect("a real model applies");

    let installed = fs::read(&dest).await.unwrap();
    assert_eq!(installed, fs::read(real_model()).await.unwrap(), "payload is live");
    let previous = fs::read(dest.with_extension("btww.previous")).await.unwrap();
    assert_eq!(previous, b"the model that was live", "the old model is retained");
  }

  #[tokio::test]
  async fn a_corrupt_model_never_displaces_a_working_one() {
    let root = temp_dir();
    let dest = root.join("hey_bridgething.btww");
    fs::write(&dest, b"the model that was live").await.unwrap();
    let junk = root.join("junk.btww");
    fs::write(&junk, b"not a model, but it did arrive intact")
      .await
      .unwrap();

    let err = apply(&junk, &dest).await.expect_err("garbage must be refused");
    assert!(matches!(err, ApplyError::Unusable(_)), "refused for being unreadable");
    assert_eq!(
      fs::read(&dest).await.unwrap(),
      b"the model that was live",
      "the live model is untouched"
    );
  }

  #[tokio::test]
  async fn a_first_install_needs_no_previous() {
    let root = temp_dir();
    let dest = root.join("nested").join("hey_bridgething.btww");

    apply(&real_model(), &dest).await.expect("applies onto an empty dir");

    assert!(fs::try_exists(&dest).await.unwrap());
    assert!(!fs::try_exists(dest.with_extension("btww.previous")).await.unwrap());
  }

  #[tokio::test]
  async fn a_refused_apply_leaves_no_staging_behind() {
    let root = temp_dir();
    let dest = root.join("hey_bridgething.btww");
    let junk = root.join("junk.btww");
    fs::write(&junk, b"not a model").await.unwrap();

    let _ = apply(&junk, &dest).await.expect_err("garbage must be refused");

    let mut rd = fs::read_dir(&root).await.unwrap();
    while let Some(entry) = rd.next_entry().await.unwrap() {
      let name = entry.file_name();
      assert!(
        !name.to_string_lossy().starts_with(".incoming."),
        "left staging behind: {name:?}"
      );
    }
  }
}
