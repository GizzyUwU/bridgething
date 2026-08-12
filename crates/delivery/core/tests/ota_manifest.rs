use std::collections::BTreeMap;

use bridgething_delivery::{
  bundle::ArtifactDigest,
  ota::manifest::{
    OtaArtifactUrls, OtaCompositeVersion, OtaDiscoverManifest, OtaReleaseArtifacts, patch_source_matches,
  },
};

const ROOT: &str = "https://ota.bridgething.com";

#[test]
fn a_composite_version_splits_into_its_daemon_and_image_halves() {
  let parsed = OtaCompositeVersion::parse("0.8.4+image.2026.05.0").expect("a well formed composite");

  assert_eq!(parsed.daemon, "0.8.4");
  assert_eq!(parsed.image, "2026.05.0");
  assert_eq!(parsed.composite(), "0.8.4+image.2026.05.0");
}

#[test]
fn a_version_missing_either_half_is_not_a_composite() {
  assert!(OtaCompositeVersion::parse("0.8.4").is_none());
  assert!(OtaCompositeVersion::parse("0.8.4+2026.05.0").is_none());
  assert!(OtaCompositeVersion::parse("+image.2026.05.0").is_none());
  assert!(OtaCompositeVersion::parse("0.8.4+image.").is_none());
}

#[test]
fn artifact_urls_are_built_from_the_channel_and_the_two_versions() {
  let urls = OtaArtifactUrls::build(ROOT, "stable", "0.8.4", "2026.05.0", "prod");

  assert_eq!(
    urls.daemon_binary,
    "https://ota.bridgething.com/daemon/stable/0.8.4/bridgething"
  );
  assert_eq!(
    urls.daemon_binary_zst,
    "https://ota.bridgething.com/daemon/stable/0.8.4/bridgething.zst"
  );
  assert_eq!(
    urls.image_swu,
    "https://ota.bridgething.com/images/stable/2026.05.0/bridgething-prod-image.swu"
  );
  assert_eq!(
    urls.image_zck,
    "https://ota.bridgething.com/images/stable/2026.05.0/bridgething-prod-image.zck"
  );
  assert_eq!(
    urls.image_boot_zck,
    "https://ota.bridgething.com/images/stable/2026.05.0/bridgething-prod-image-boot.zck"
  );
}

#[test]
fn a_webapp_and_a_daemon_patch_have_their_own_shapes() {
  assert_eq!(
    OtaArtifactUrls::builtin_webapp(ROOT, "stable", "hub", "0.1.0"),
    "https://ota.bridgething.com/webapps/stable/hub/0.1.0/hub.zip"
  );
  assert_eq!(
    OtaArtifactUrls::daemon_patch(ROOT, "stable", "0.8.4", "0.8.3"),
    "https://ota.bridgething.com/daemon/stable/0.8.4/patches/from-0.8.3.zst"
  );
}

#[test]
fn a_wake_word_model_lives_under_its_own_channel_and_version() {
  assert_eq!(
    OtaArtifactUrls::wakeword_model(ROOT, "stable", "1.2.0"),
    "https://ota.bridgething.com/wakeword/stable/model/1.2.0/hey_bridgething.btww"
  );
  assert_eq!(
    OtaArtifactUrls::wakeword_model("https://ota.bridgething.com/", "stable", "1.2.0"),
    OtaArtifactUrls::wakeword_model(ROOT, "stable", "1.2.0")
  );
}

#[test]
fn a_release_carries_the_wake_word_model_the_site_publishes() {
  let raw = r#"
  {
    "manifest_version": 1,
    "updated_at": "2026-05-30T00:00:00Z",
    "channels": {
      "stable": {"name": "stable", "stability": "stable", "default": true, "latest": "0.8.4+image.2026.05.0", "releases": ["0.8.4+image.2026.05.0"]}
    },
    "releases": {
      "0.8.4+image.2026.05.0": {
        "version": "0.8.4+image.2026.05.0", "channel": "stable", "deprecated": false,
        "wakeword": {"runtime": "0.3.0", "model": "1.2.0", "model_trained_against": {"runtime": "0.3.0"}},
        "artifacts": {"wakeword": {"model": {"size": 1234, "sha256": "abc"}}}
      }
    }
  }
  "#;

  let manifest: OtaDiscoverManifest = serde_json::from_str(raw).expect("the manifest decodes");
  let release = manifest
    .releases
    .get("0.8.4+image.2026.05.0")
    .expect("the latest release");

  let wakeword = release.wakeword.as_ref().expect("the release declares a wake word");
  assert_eq!(wakeword.model, "1.2.0");
  assert_eq!(wakeword.runtime, "0.3.0");

  let published = release
    .artifacts
    .as_ref()
    .and_then(|artifacts| artifacts.wakeword.as_ref())
    .and_then(|wakeword| wakeword.model.as_ref())
    .expect("the model digest");
  assert_eq!(published.size, 1234);
  assert_eq!(published.sha256, "abc");
}

#[test]
fn a_release_without_a_wake_word_still_decodes() {
  let raw = r#"
  {
    "manifest_version": 1,
    "updated_at": "2026-05-30T00:00:00Z",
    "channels": {
      "stable": {"name": "stable", "stability": "stable", "default": true, "latest": "0.8.4+image.2026.05.0", "releases": ["0.8.4+image.2026.05.0"]}
    },
    "releases": {
      "0.8.4+image.2026.05.0": {"version": "0.8.4+image.2026.05.0", "channel": "stable", "deprecated": false}
    }
  }
  "#;

  let manifest: OtaDiscoverManifest = serde_json::from_str(raw).expect("the manifest decodes");
  assert!(
    manifest
      .releases
      .get("0.8.4+image.2026.05.0")
      .expect("the release")
      .wakeword
      .is_none()
  );
}

#[test]
fn a_trailing_slash_on_the_root_does_not_double_up() {
  let slashed = OtaArtifactUrls::build("https://ota.bridgething.com/", "stable", "0.8.4", "2026.05.0", "prod");

  assert_eq!(
    slashed.daemon_binary,
    OtaArtifactUrls::build(ROOT, "stable", "0.8.4", "2026.05.0", "prod").daemon_binary
  );
  assert_eq!(
    OtaArtifactUrls::builtin_webapp("https://ota.bridgething.com/", "stable", "hub", "0.1.0"),
    OtaArtifactUrls::builtin_webapp(ROOT, "stable", "hub", "0.1.0")
  );
}

#[test]
fn the_manifest_decodes_channels_and_releases() {
  let raw = r#"
  {
    "manifest_version": 1,
    "updated_at": "2026-05-30T00:00:00Z",
    "channels": {
      "stable": {
        "name": "stable", "stability": "stable", "default": true,
        "latest": "0.8.4+image.2026.05.0",
        "releases": ["0.8.4+image.2026.05.0", "0.8.3+image.2026.04.0"]
      }
    },
    "releases": {
      "0.8.4+image.2026.05.0": {"version": "0.8.4+image.2026.05.0", "channel": "stable", "deprecated": false, "builtin_webapps": {"hub": "0.1.0", "stock": "8.9.2"}},
      "0.8.3+image.2026.04.0": {"version": "0.8.3+image.2026.04.0", "channel": "stable", "yanked": "bad build", "deprecated": false}
    }
  }
  "#;

  let manifest: OtaDiscoverManifest = serde_json::from_str(raw).expect("the manifest decodes");
  let stable = manifest.channels.get("stable").expect("the stable channel");

  assert_eq!(manifest.manifest_version, 1);
  assert_eq!(manifest.updated_at, "2026-05-30T00:00:00Z");
  assert!(stable.is_default);
  assert_eq!(stable.latest, "0.8.4+image.2026.05.0");
  assert_eq!(stable.releases.len(), 2);

  let latest = manifest
    .releases
    .get("0.8.4+image.2026.05.0")
    .expect("the latest release");
  let older = manifest
    .releases
    .get("0.8.3+image.2026.04.0")
    .expect("the older release");

  assert!(latest.yanked.is_none());
  assert_eq!(older.yanked.as_deref(), Some("bad build"));
  assert_eq!(
    latest.builtin_webapps,
    BTreeMap::from([
      ("hub".to_string(), "0.1.0".to_string()),
      ("stock".to_string(), "8.9.2".to_string())
    ])
  );
  assert!(
    older.builtin_webapps.is_empty(),
    "an absent webapp map is empty, not a decode failure"
  );
  assert!(older.artifacts.is_none());
}

#[test]
fn a_daemon_patch_carries_the_source_digest_it_applies_to() {
  let raw = r#"
  {
    "daemon": {"size": 100, "sha256": "aa"},
    "daemon_patches": {
      "0.8.3": {"size": 10, "sha256": "bb", "source_sha256": "cc"},
      "0.8.2": {"size": 20, "sha256": "dd"}
    }
  }
  "#;

  let artifacts: OtaReleaseArtifacts = serde_json::from_str(raw).expect("the artifacts decode");
  let from_83 = artifacts.daemon_patches.get("0.8.3").expect("the 0.8.3 patch");
  let from_82 = artifacts.daemon_patches.get("0.8.2").expect("the 0.8.2 patch");

  assert_eq!(
    artifacts.daemon,
    Some(ArtifactDigest {
      size: 100,
      sha256: "aa".into()
    })
  );
  assert_eq!(from_83.source_sha256.as_deref(), Some("cc"));
  assert_eq!(
    from_83.digest(),
    ArtifactDigest {
      size: 10,
      sha256: "bb".into()
    }
  );
  assert!(from_82.source_sha256.is_none());
  assert!(artifacts.webapps.is_empty());
  assert!(artifacts.image_swu.is_none());
}

#[test]
fn a_patch_applies_only_when_the_source_it_names_is_what_is_running() {
  assert!(patch_source_matches(Some("abc"), Some("abc")));
  assert!(
    patch_source_matches(Some("ABC"), Some("abc")),
    "a digest is hex, so its case carries no meaning"
  );
  assert!(!patch_source_matches(Some("abc"), Some("def")));
  assert!(
    patch_source_matches(None, Some("abc")),
    "an unstated source is not a mismatch"
  );
  assert!(patch_source_matches(Some("abc"), None));
  assert!(patch_source_matches(None, None));
}
