use std::path::{Path, PathBuf};

use crate::{auth::TokenStore, http::random_hex};

const REFRESH_TOKEN_FILE: &str = ".refresh_token.txt";
const USERNAME_FILE: &str = ".username";
const DEVICE_ID_FILE: &str = ".device_id";

pub struct FileTokenStore {
  dir: PathBuf,
}

impl FileTokenStore {
  pub fn new(dir: impl Into<PathBuf>) -> std::io::Result<Self> {
    let dir = dir.into();
    std::fs::create_dir_all(&dir)?;
    Ok(FileTokenStore { dir })
  }

  fn read(&self, name: &str) -> Option<String> {
    std::fs::read_to_string(self.dir.join(name))
      .ok()
      .map(|s| s.trim().to_string())
      .filter(|s| !s.is_empty())
  }

  fn write(&self, name: &str, value: &str) -> std::io::Result<()> {
    let target = self.dir.join(name);
    let tmp = self.dir.join(format!("{name}.tmp"));
    std::fs::write(&tmp, value)?;
    std::fs::rename(&tmp, &target)
  }
}

pub fn load_or_make_device_id(dir: &Path) -> String {
  let p = dir.join(DEVICE_ID_FILE);
  if let Ok(s) = std::fs::read_to_string(&p) {
    let s = s.trim().to_string();
    if !s.is_empty() {
      return s;
    }
  }
  let id = random_hex(20);
  if let Err(err) = std::fs::write(&p, &id) {
    tracing::warn!(path = %p.display(), %err, "spotify: device id not persisted; the next start will look like a new device");
  }
  id
}

impl TokenStore for FileTokenStore {
  fn load_refresh_token(&self) -> Option<String> {
    self.read(REFRESH_TOKEN_FILE)
  }
  fn save_refresh_token(&self, token: String) {
    if let Err(err) = self.write(REFRESH_TOKEN_FILE, &token) {
      tracing::error!(dir = %self.dir.display(), %err, "spotify: rotated refresh token not persisted");
    }
  }
  fn load_username(&self) -> Option<String> {
    self.read(USERNAME_FILE)
  }
  fn save_username(&self, username: String) {
    if let Err(err) = self.write(USERNAME_FILE, &username) {
      tracing::warn!(dir = %self.dir.display(), %err, "spotify: username not persisted");
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_persist_that_cannot_land_reports_instead_of_swallowing() {
    let dir = std::env::temp_dir().join("sfp-store-test-unwritable");
    let _ = std::fs::remove_dir_all(&dir);
    let store = FileTokenStore::new(&dir).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();

    let err = store
      .write(REFRESH_TOKEN_FILE, "rt-123")
      .expect_err("a write into a vanished directory cannot succeed");
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
  }

  #[test]
  fn roundtrips_and_rename_leaves_no_temp() {
    let dir = std::env::temp_dir().join("sfp-store-test-roundtrip");
    let _ = std::fs::remove_dir_all(&dir);
    let store = FileTokenStore::new(&dir).unwrap();
    assert!(store.load_refresh_token().is_none());
    store.save_refresh_token("rt-123".to_string());
    assert_eq!(store.load_refresh_token().as_deref(), Some("rt-123"));
    store.save_refresh_token("rt-456".to_string());
    assert_eq!(store.load_refresh_token().as_deref(), Some("rt-456"));
    assert!(!dir.join(".refresh_token.txt.tmp").exists());
    let _ = std::fs::remove_dir_all(&dir);
  }
}
