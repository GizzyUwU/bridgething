use std::{path::Path, sync::Arc};

use nlu::{DecodedFrame, ManifestInfo, NluDecoder, NluError, TokenizedInput};

pub use crate::voice::rejection::InferenceOutput;
use crate::{
  backend::{NluModelRunner, NluRunnerError},
  voice::{intent_catalog, rejection::RejectionPolicy, slot_mapping},
};

#[derive(Debug, thiserror::Error)]
pub enum InferError {
  #[error(transparent)]
  Bundle(#[from] NluError),
  #[error(transparent)]
  Runner(#[from] NluRunnerError),
  #[error("{0}")]
  Runtime(String),
}

#[async_trait::async_trait]
pub trait NluInference: Send + Sync {
  async fn prewarm(&self);
  async fn infer(&self, transcript: &str) -> Result<InferenceOutput, InferError>;
}

pub trait NluDecoding: Send + Sync {
  fn info(&self) -> ManifestInfo;
  fn tokenize(&self, transcript: String) -> Result<TokenizedInput, NluError>;
  fn decode(
    &self,
    transcript: String,
    tokens: TokenizedInput,
    intent_logits: Vec<f32>,
    bio_logits: Vec<f32>,
    closed_logits: Vec<Vec<f32>>,
  ) -> Result<DecodedFrame, NluError>;
}

impl NluDecoding for NluDecoder {
  fn info(&self) -> ManifestInfo {
    NluDecoder::info(self)
  }

  fn tokenize(&self, transcript: String) -> Result<TokenizedInput, NluError> {
    NluDecoder::tokenize(self, &transcript)
  }

  fn decode(
    &self,
    transcript: String,
    tokens: TokenizedInput,
    intent_logits: Vec<f32>,
    bio_logits: Vec<f32>,
    closed_logits: Vec<Vec<f32>>,
  ) -> Result<DecodedFrame, NluError> {
    NluDecoder::decode(self, &transcript, &tokens, &intent_logits, &bio_logits, &closed_logits)
  }
}

#[derive(Debug, thiserror::Error)]
pub enum BundleError {
  #[error("bundle intents {bundle:?} do not match the companion catalog {catalog:?}")]
  CatalogMismatch { bundle: Vec<String>, catalog: Vec<String> },
  #[error(transparent)]
  Load(#[from] NluError),
}

pub struct BundleInference {
  decoder: Arc<dyn NluDecoding>,
  runner: Arc<dyn NluModelRunner>,
  rejection: Option<RejectionPolicy>,
}

pub fn check_catalog(info: &ManifestInfo) -> Result<(), BundleError> {
  if info
    .intent_names
    .iter()
    .map(String::as_str)
    .ne(intent_catalog::SURFACE_NAMES.iter().copied())
  {
    return Err(BundleError::CatalogMismatch {
      bundle: info.intent_names.clone(),
      catalog: intent_catalog::SURFACE_NAMES.iter().map(|n| (*n).to_owned()).collect(),
    });
  }
  Ok(())
}

impl BundleInference {
  pub fn new(decoder: Arc<dyn NluDecoding>, runner: Arc<dyn NluModelRunner>) -> Result<Self, BundleError> {
    let info = decoder.info();
    check_catalog(&info)?;
    let rejection = info.rejection.map(|sweep| RejectionPolicy {
      in_domain_threshold: sweep.in_domain_threshold,
      clarify_margin: sweep.clarify_margin,
      ..RejectionPolicy::default()
    });
    Ok(Self {
      decoder,
      runner,
      rejection,
    })
  }

  pub fn load(bundle_dir: &Path, runner: Arc<dyn NluModelRunner>) -> Result<Self, BundleError> {
    Self::new(Arc::new(NluDecoder::load(bundle_dir)?), runner)
  }

  pub fn rejection(&self) -> Option<RejectionPolicy> {
    self.rejection
  }
}

#[async_trait::async_trait]
impl NluInference for BundleInference {
  async fn prewarm(&self) {
    let runner = self.runner.clone();
    if let Err(error) = tokio::task::spawn_blocking(move || runner.prewarm()).await {
      tracing::warn!(%error, "prewarm did not finish");
    }
  }

  async fn infer(&self, transcript: &str) -> Result<InferenceOutput, InferError> {
    let decoder = self.decoder.clone();
    let runner = self.runner.clone();
    let transcript = transcript.to_owned();
    tokio::task::spawn_blocking(move || {
      let tokens = decoder.tokenize(transcript.clone())?;
      let heads = runner.predict(tokens.input_ids.clone(), tokens.attention_mask.clone())?;
      let frame = decoder.decode(
        transcript,
        tokens,
        heads.intent_logits.clone(),
        heads.bio_logits,
        heads.closed_logits,
      )?;
      Ok(InferenceOutput {
        intent_logits: heads.intent_logits.iter().map(|logit| *logit as f64).collect(),
        in_domain_logit: -(heads.ood_logit as f64),
        slots: slot_mapping::apply(&frame.slots),
      })
    })
    .await
    .map_err(|error| InferError::Runtime(error.to_string()))?
  }
}
