#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct NluModelOutputs {
  pub intent_logits: Vec<f32>,
  pub ood_logit: f32,
  pub bio_logits: Vec<f32>,
  pub closed_logits: Vec<Vec<f32>>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum NluRunnerError {
  #[error("model is not loaded")]
  NotLoaded,
  #[error("inference failed: {reason}")]
  Failed { reason: String },
}

#[uniffi::export(with_foreign)]
pub trait NluModelRunner: Send + Sync {
  fn prewarm(&self);
  fn predict(&self, input_ids: Vec<i32>, attention_mask: Vec<i32>) -> Result<NluModelOutputs, NluRunnerError>;
}
