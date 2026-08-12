#[uniffi::export(with_foreign)]
pub trait SecretStore: Send + Sync {
  fn get(&self, key: String) -> Option<String>;
  fn set(&self, key: String, value: String);
  fn remove(&self, key: String);
  fn get_blob(&self, key: String) -> Option<Vec<u8>>;
}
