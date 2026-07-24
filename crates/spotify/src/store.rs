use std::path::{Path, PathBuf};

use crate::{auth::TokenStore, http::random_hex};

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

  fn write(&self, name: &str, value: &str) {
    let target = self.dir.join(name);
    let tmp = self.dir.join(format!("{name}.tmp"));
    if std::fs::write(&tmp, value).is_ok() {
      let _ = std::fs::rename(&tmp, &target);
    }
  }
}

pub fn load_or_make_device_id(dir: &Path) -> String {
  let p = dir.join(".device_id");
  if let Ok(s) = std::fs::read_to_string(&p) {
    let s = s.trim().to_string();
    if !s.is_empty() {
      return s;
    }
  }
  let id = random_hex(20);
  let _ = std::fs::write(&p, &id);
  id
}

impl TokenStore for FileTokenStore {
  fn load_refresh_token(&self) -> Option<String> {
    self.read(".refresh_token.txt")
  }
  fn save_refresh_token(&self, token: String) {
    self.write(".refresh_token.txt", &token);
  }
  fn load_username(&self) -> Option<String> {
    self.read(".username")
  }
  fn save_username(&self, username: String) {
    self.write(".username", &username);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

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
