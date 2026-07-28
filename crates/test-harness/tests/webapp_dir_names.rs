use std::path::{Path, PathBuf};

use bridgething_test_harness::Harness;
use uuid::Uuid;

fn plant_at(harness: &Harness, dir_name: &str, id: Uuid, marker: &str) -> PathBuf {
  let dir = harness.state_dir().join("webapps").join(dir_name);
  std::fs::create_dir_all(&dir).expect("bundle dir");
  std::fs::write(dir.join("index.html"), marker.as_bytes()).expect("index");
  std::fs::write(
    dir.join("manifest.json"),
    format!(r#"{{"id":"{id}","name":"planted","version":"0.1.0"}}"#),
  )
  .expect("manifest");
  dir
}

fn canonical(root: &Path, id: Uuid) -> PathBuf {
  root.join("webapps").join(id.simple().to_string())
}

fn marker_at(dir: &Path) -> Option<String> {
  std::fs::read_to_string(dir.join("index.html")).ok()
}

#[tokio::test]
async fn a_bundle_under_a_non_canonical_name_is_moved_to_its_canonical_one() {
  let harness = Harness::start().await.expect("harness start");
  let id = Uuid::now_v7();
  plant_at(&harness, &id.hyphenated().to_string(), id, "pushed");

  harness.state().webapps.rescan().await;

  let dest = canonical(harness.state_dir(), id);
  assert_eq!(
    marker_at(&dest).as_deref(),
    Some("pushed"),
    "the bundle should now live under its canonical name"
  );
  assert!(
    !harness
      .state_dir()
      .join("webapps")
      .join(id.hyphenated().to_string())
      .exists(),
    "the non-canonical directory should be gone"
  );
  assert!(
    harness.state().webapps.bundle(id).await.is_some(),
    "the renamed bundle should still be registered"
  );
}

#[tokio::test]
async fn a_duplicate_never_evicts_the_bundle_already_installed_correctly() {
  let harness = Harness::start().await.expect("harness start");
  let id = Uuid::now_v7();
  plant_at(&harness, &id.simple().to_string(), id, "installed");
  plant_at(&harness, &id.hyphenated().to_string(), id, "duplicate");

  harness.state().webapps.rescan().await;

  let dest = canonical(harness.state_dir(), id);
  assert_eq!(
    marker_at(&dest).as_deref(),
    Some("installed"),
    "the copy under the canonical name is the one that survives"
  );
  assert!(
    !harness
      .state_dir()
      .join("webapps")
      .join(id.hyphenated().to_string())
      .exists(),
    "the duplicate should be discarded, not left to shadow"
  );
}

#[tokio::test]
async fn a_half_written_duplicate_cannot_replace_a_working_install() {
  let harness = Harness::start().await.expect("harness start");
  let id = Uuid::now_v7();
  plant_at(&harness, &id.simple().to_string(), id, "installed");

  let partial = plant_at(&harness, &id.hyphenated().to_string(), id, "");
  std::fs::write(partial.join("index.html"), b"").expect("truncated index");

  harness.state().webapps.rescan().await;

  assert_eq!(
    marker_at(&canonical(harness.state_dir(), id)).as_deref(),
    Some("installed"),
    "a bundle the daemon never wrote must not be able to destroy one it did"
  );
}

#[tokio::test]
async fn two_bundles_keep_their_own_directories() {
  let harness = Harness::start().await.expect("harness start");
  let first = Uuid::now_v7();
  let second = Uuid::now_v7();
  plant_at(&harness, &first.hyphenated().to_string(), first, "first");
  plant_at(&harness, "some-hand-made-name", second, "second");

  harness.state().webapps.rescan().await;

  assert_eq!(
    marker_at(&canonical(harness.state_dir(), first)).as_deref(),
    Some("first")
  );
  assert_eq!(
    marker_at(&canonical(harness.state_dir(), second)).as_deref(),
    Some("second")
  );
  assert!(harness.state().webapps.bundle(first).await.is_some());
  assert!(harness.state().webapps.bundle(second).await.is_some());
}

#[tokio::test]
async fn a_directory_without_a_readable_manifest_is_left_alone() {
  let harness = Harness::start().await.expect("harness start");
  let dir = harness.state_dir().join("webapps").join("not-a-bundle");
  std::fs::create_dir_all(&dir).expect("dir");
  std::fs::write(dir.join("index.html"), b"orphan").expect("index");
  std::fs::write(dir.join("manifest.json"), b"{ not json").expect("manifest");

  harness.state().webapps.rescan().await;

  assert_eq!(
    marker_at(&dir).as_deref(),
    Some("orphan"),
    "nothing can be inferred about where it belongs, so it stays put"
  );
}
