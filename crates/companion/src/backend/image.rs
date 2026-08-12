#[uniffi::export(with_foreign)]
pub trait ImageScaler: Send + Sync {
  fn downsample_jpeg(&self, bytes: Vec<u8>, max_edge: u32, quality: f32) -> Option<Vec<u8>>;
}
