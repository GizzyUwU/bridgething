pub type Result<T> = std::result::Result<T, NluError>;

#[derive(Debug, thiserror::Error)]
pub enum NluError {
  #[error("bundle load failed: {msg}")]
  BundleLoad { msg: String },
  #[error("manifest invalid: {msg}")]
  ManifestInvalid { msg: String },
  #[error("tokenizer failed: {msg}")]
  Tokenizer { msg: String },
  #[error("model output shape mismatch: {msg}")]
  ShapeMismatch { msg: String },
}
