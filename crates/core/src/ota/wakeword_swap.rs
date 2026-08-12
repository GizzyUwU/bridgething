use std::{
  cmp::Ordering,
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

/// The .btww container has no version field, so the version rides beside the model it names.
pub fn version_path(model: &Path) -> PathBuf {
  model.with_extension("version")
}

/// The version of the model the daemon would load, read from the same candidate order the mic walks.
///
/// The first model that exists answers, stamp or no stamp: a later candidate's version would name a
/// model this daemon is not running.
pub async fn installed_version() -> Option<String> {
  stamp_of(&first_installed().await?).await
}

async fn first_installed() -> Option<PathBuf> {
  for model in crate::paths::wakeword_models() {
    if fs::try_exists(&model).await.unwrap_or(false) {
      return Some(model);
    }
  }
  None
}

/// Drop a data-partition model the image's floor copy has superseded.
///
/// The data copy wins the path walk and an image ota never rewrites it, so a model that arrived by
/// ota outlives the embedding graph it was trained against, and a graph the daemon has since
/// replaced pairs with it into a wake word that never fires. Must run before anything reads a model.
///
/// Only positive evidence retires a model: both versions parse and the floor is strictly newer.
/// Equal keeps the data copy, since the versions name the same model and it is the one the ota path
/// owns. Anything unprovable keeps it too - the floor is read-only and still there to fall back to,
/// but a model deleted on a guess is gone, and an unstamped copy is as likely to be a hand-pushed
/// one as a stale one.
pub async fn retire_superseded() {
  let models = crate::paths::wakeword_models();
  let (Some(live), Some(floor)) = (models.first(), models.last()) else {
    return;
  };
  if live != floor {
    retire_if_superseded(live, floor).await;
  }
}

async fn retire_if_superseded(live: &Path, floor: &Path) {
  let (Some(live_version), Some(floor_version)) = (stamp_of(live).await, stamp_of(floor).await) else {
    return;
  };
  if version_cmp(&floor_version, &live_version) != Some(Ordering::Greater) {
    return;
  }

  tracing::info!(
    model = %live.display(),
    retired = %live_version,
    floor = %floor_version,
    "retiring a wake word model the image's floor copy supersedes"
  );
  staging::remove_any(live).await;
  staging::remove_any(&version_path(live)).await;
}

async fn stamp_of(model: &Path) -> Option<String> {
  if !fs::try_exists(model).await.unwrap_or(false) {
    return None;
  }
  let stamp = fs::read_to_string(version_path(model)).await.ok()?;
  let stamp = stamp.trim();
  (!stamp.is_empty()).then(|| stamp.to_owned())
}

/// Order two dotted numeric versions. None when either side is not purely numeric, which the caller
/// treats as no evidence rather than guessing an order.
fn version_cmp(a: &str, b: &str) -> Option<Ordering> {
  let parse = |v: &str| {
    v.split('.')
      .map(|p| p.parse::<u64>())
      .collect::<Result<Vec<_>, _>>()
      .ok()
  };
  let (a, b) = (parse(a)?, parse(b)?);
  let width = a.len().max(b.len());
  Some(
    (0..width)
      .map(|i| (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0)))
      .fold(Ordering::Equal, |acc, (x, y)| acc.then(x.cmp(&y))),
  )
}

pub async fn apply(payload: &Path, dest: &Path, version: Option<&str>) -> Result<(), ApplyError> {
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

  // a stamp naming the model that is no longer there is worse than no stamp, so an unversioned
  // push clears it rather than leaving the old one to be reported as this model's version.
  let stamp = version_path(dest);
  match version {
    Some(version) => {
      if let Err(err) = fs::write(&stamp, format!("{version}\n")).await {
        tracing::warn!(path = %stamp.display(), %err, "could not stamp the wake word model version");
        staging::remove_any(&stamp).await;
      }
    }
    None => staging::remove_any(&stamp).await,
  }

  tracing::info!(model = %dest.display(), ?version, "wake word model applied");
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

    apply(&real_model(), &dest, Some("1.2.0"))
      .await
      .expect("a real model applies");

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

    let err = apply(&junk, &dest, Some("1.2.0"))
      .await
      .expect_err("garbage must be refused");
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

    apply(&real_model(), &dest, Some("1.2.0"))
      .await
      .expect("applies onto an empty dir");

    assert!(fs::try_exists(&dest).await.unwrap());
    assert!(!fs::try_exists(dest.with_extension("btww.previous")).await.unwrap());
  }

  async fn stamp_of(dest: &Path) -> Option<String> {
    fs::read_to_string(version_path(dest))
      .await
      .ok()
      .map(|s| s.trim().to_owned())
  }

  #[tokio::test]
  async fn the_pushed_version_is_stamped_beside_the_model() {
    let root = temp_dir();
    let dest = root.join("hey_bridgething.btww");

    apply(&real_model(), &dest, Some("1.2.0")).await.unwrap();

    assert_eq!(stamp_of(&dest).await.as_deref(), Some("1.2.0"));
  }

  #[tokio::test]
  async fn an_unversioned_push_clears_the_stamp_of_the_model_it_replaced() {
    let root = temp_dir();
    let dest = root.join("hey_bridgething.btww");
    apply(&real_model(), &dest, Some("1.2.0")).await.unwrap();

    apply(&real_model(), &dest, None).await.unwrap();

    assert_eq!(
      stamp_of(&dest).await,
      None,
      "a stale version would be reported as this model's"
    );
  }

  #[tokio::test]
  async fn a_refused_apply_leaves_the_stamp_alone() {
    let root = temp_dir();
    let dest = root.join("hey_bridgething.btww");
    apply(&real_model(), &dest, Some("1.2.0")).await.unwrap();
    let junk = root.join("junk.btww");
    fs::write(&junk, b"not a model").await.unwrap();

    let _ = apply(&junk, &dest, Some("9.9.9"))
      .await
      .expect_err("garbage must be refused");

    assert_eq!(
      stamp_of(&dest).await.as_deref(),
      Some("1.2.0"),
      "the live model kept its version"
    );
  }

  #[tokio::test]
  async fn a_refused_apply_leaves_no_staging_behind() {
    let root = temp_dir();
    let dest = root.join("hey_bridgething.btww");
    let junk = root.join("junk.btww");
    fs::write(&junk, b"not a model").await.unwrap();

    let _ = apply(&junk, &dest, Some("1.2.0"))
      .await
      .expect_err("garbage must be refused");

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

// retirement compares stamps and touches files; none of it needs a wake word runtime, so unlike the
// apply tests it runs on any host rather than only where the mic feature builds.
#[cfg(test)]
mod retire_tests {
  use super::*;

  fn temp_dir() -> PathBuf {
    let p = std::env::temp_dir().join(format!("bridgething-wakeword-retire-{}", Uuid::now_v7().simple()));
    std::fs::create_dir_all(&p).unwrap();
    p
  }

  // live = the ota-writable copy on the data partition, floor = the image's read-only copy.
  async fn pair(live_version: Option<&str>, floor_version: Option<&str>) -> (PathBuf, PathBuf) {
    let root = temp_dir();
    let live = root.join("var/hey_bridgething.btww");
    let floor = root.join("usr/hey_bridgething.btww");
    for (model, version) in [(&live, live_version), (&floor, floor_version)] {
      fs::create_dir_all(model.parent().unwrap()).await.unwrap();
      fs::write(model, b"a model").await.unwrap();
      if let Some(version) = version {
        fs::write(version_path(model), format!("{version}\n")).await.unwrap();
      }
    }
    (live, floor)
  }

  #[tokio::test]
  async fn a_newer_image_floor_retires_the_model_the_data_partition_kept() {
    let (live, floor) = pair(Some("1.0.0"), Some("1.1.0")).await;

    retire_if_superseded(&live, &floor).await;

    assert!(!fs::try_exists(&live).await.unwrap(), "the superseded model is gone");
    assert!(
      !fs::try_exists(version_path(&live)).await.unwrap(),
      "its stamp went with it"
    );
    assert!(fs::try_exists(&floor).await.unwrap(), "the floor is untouched");
  }

  #[tokio::test]
  async fn a_newer_data_model_outlives_an_older_image_floor() {
    let (live, floor) = pair(Some("2.0.0"), Some("1.1.0")).await;

    retire_if_superseded(&live, &floor).await;

    assert!(fs::try_exists(&live).await.unwrap());
  }

  // equal names the same model, and the data copy is the one the ota path owns.
  #[tokio::test]
  async fn equal_versions_keep_the_data_copy() {
    let (live, floor) = pair(Some("1.1.0"), Some("1.1.0")).await;

    retire_if_superseded(&live, &floor).await;

    assert!(fs::try_exists(&live).await.unwrap());
  }

  #[tokio::test]
  async fn an_unstamped_data_model_is_never_retired_on_a_guess() {
    let (live, floor) = pair(None, Some("9.9.9")).await;

    retire_if_superseded(&live, &floor).await;

    assert!(
      fs::try_exists(&live).await.unwrap(),
      "a hand-pushed model must survive a boot"
    );
  }

  #[tokio::test]
  async fn an_unstamped_floor_retires_nothing() {
    let (live, floor) = pair(Some("1.0.0"), None).await;

    retire_if_superseded(&live, &floor).await;

    assert!(fs::try_exists(&live).await.unwrap());
  }

  #[tokio::test]
  async fn a_floor_that_does_not_exist_retires_nothing() {
    let (live, floor) = pair(Some("1.0.0"), Some("9.9.9")).await;
    fs::remove_file(&floor).await.unwrap();

    retire_if_superseded(&live, &floor).await;

    assert!(
      fs::try_exists(&live).await.unwrap(),
      "nothing to fall back to, so nothing is retired"
    );
  }

  #[tokio::test]
  async fn an_unparseable_version_is_no_evidence() {
    let (live, floor) = pair(Some("1.0.0"), Some("1.1.0-rc1")).await;

    retire_if_superseded(&live, &floor).await;

    assert!(fs::try_exists(&live).await.unwrap());
    assert_eq!(version_cmp("1.1.0-rc1", "1.0.0"), None);
  }

  #[test]
  fn versions_order_numerically_not_lexically() {
    assert_eq!(version_cmp("1.10.0", "1.9.0"), Some(Ordering::Greater));
    assert_eq!(version_cmp("1.2", "1.2.0"), Some(Ordering::Equal));
    assert_eq!(version_cmp("2.0.0", "10.0.0"), Some(Ordering::Less));
    assert_eq!(version_cmp("1.0.0", ""), None);
  }
}
