use std::path::Path;

use bridgething_companion::api::CapabilityFlags;
use serde_json::Value;

use crate::store::{JsonFile, stored};

pub struct Capabilities(JsonFile<CapabilityFlags>);

impl Capabilities {
  pub fn open(config_dir: &Path, fresh: CapabilityFlags) -> Self {
    let path = config_dir.join("capabilities.json");
    let held = stored::<Value>(&path).map_or(fresh, |body| merge(fresh, body));
    Self(JsonFile::new(path, "capability choices", held))
  }

  pub fn get(&self) -> CapabilityFlags {
    self.0.read(|held| *held)
  }

  pub fn set(&self, flags: CapabilityFlags) {
    self.0.set(flags);
  }
}

fn merge(fresh: CapabilityFlags, stored: Value) -> CapabilityFlags {
  let (Ok(Value::Object(mut merged)), Value::Object(stored)) = (serde_json::to_value(fresh), stored) else {
    return fresh;
  };
  merged.extend(stored);
  serde_json::from_value(Value::Object(merged)).unwrap_or(fresh)
}

#[cfg(test)]
mod tests {
  use std::fs;

  use super::*;

  const FRESH: CapabilityFlags = CapabilityFlags {
    geo: true,
    notifications: false,
    net_fetch: true,
    net_ws: true,
    audio_tts: true,
    voice_model: false,
  };

  #[test]
  fn a_fresh_host_offers_everything_it_can_serve() {
    let dir = tempfile::tempdir().expect("a scratch directory");

    assert_eq!(
      Capabilities::open(dir.path(), FRESH).get(),
      FRESH,
      "nothing on disk means the platform defaults stand"
    );
  }

  #[test]
  fn a_refusal_outlives_the_process_that_made_it() {
    let dir = tempfile::tempdir().expect("a scratch directory");

    let first = Capabilities::open(dir.path(), FRESH);
    first.set(CapabilityFlags { geo: false, ..FRESH });

    assert!(
      !Capabilities::open(dir.path(), FRESH).get().geo,
      "a capability turned off does not come back on at the next launch"
    );
  }

  #[test]
  fn a_capability_the_stored_file_predates_takes_the_fresh_default() {
    let dir = tempfile::tempdir().expect("a scratch directory");
    fs::write(
      dir.path().join("capabilities.json"),
      br#"{"geo":false,"netFetch":true}"#,
    )
    .expect("a stored file from an older build");

    let held = Capabilities::open(dir.path(), FRESH).get();

    assert!(!held.geo, "what the file does say is honored");
    assert!(held.audio_tts, "what it does not say falls back to the fresh default");
  }
}
