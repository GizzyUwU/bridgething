use std::sync::Mutex;

use bridgething_companion::backend::SecretStore;

#[derive(Default)]
pub struct MemorySecrets(Mutex<std::collections::HashMap<String, String>>);

impl SecretStore for MemorySecrets {
  fn get(&self, key: String) -> Option<String> {
    self.0.lock().unwrap().get(&key).cloned()
  }
  fn set(&self, key: String, value: String) {
    self.0.lock().unwrap().insert(key, value);
  }
  fn remove(&self, key: String) {
    self.0.lock().unwrap().remove(&key);
  }
  fn get_blob(&self, key: String) -> Option<Vec<u8>> {
    self.get(key).map(String::into_bytes)
  }
}
