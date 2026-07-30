pub mod features;

use std::path::Path;

use features::{EMBEDDING_DIM, Features};
use tract_onnx::prelude::*;

#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error("loading {0}: {1}")]
  Model(String, String),
  #[error("inference: {0}")]
  Infer(String),
}

type Runnable = std::sync::Arc<TypedRunnableModel>;

pub const CLASSIFIER_FRAMES: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Detection {
  pub score: f32,
  pub at_sample: u64,
}

pub struct WakeWord {
  features: Features,
  classifier: Runnable,
  threshold: f32,
  refractory_chunks: usize,
  quiet_for: usize,
  consumed: u64,
}

impl WakeWord {
  pub fn new(models: &Path, phrase_model: &Path, threshold: f32) -> Result<Self, Error> {
    let classifier = tract_onnx::onnx()
      .model_for_path(phrase_model)
      .map_err(|e| Error::Model(phrase_model.display().to_string(), e.to_string()))?
      .with_input_fact(0, f32::fact([1, CLASSIFIER_FRAMES, EMBEDDING_DIM]).into())
      .map_err(|e| Error::Model(phrase_model.display().to_string(), e.to_string()))?
      .into_optimized()
      .map_err(|e| Error::Model(phrase_model.display().to_string(), e.to_string()))?
      .into_runnable()
      .map_err(|e| Error::Model(phrase_model.display().to_string(), e.to_string()))?;

    Ok(Self {
      features: Features::new(
        &models.join("melspectrogram.onnx"),
        &models.join("embedding_model.onnx"),
      )?,
      classifier,
      threshold,
      refractory_chunks: 25,
      quiet_for: 0,
      consumed: 0,
    })
  }

  pub fn threshold(&self) -> f32 {
    self.threshold
  }

  pub fn set_threshold(&mut self, threshold: f32) {
    self.threshold = threshold;
  }

  pub fn push(&mut self, samples: &[f32]) -> Result<Option<Detection>, Error> {
    let produced = self.features.push(samples)?;
    self.consumed += samples.len() as u64;

    let mut hit = None;
    for _ in 0..produced {
      let score = self.score()?;
      if self.quiet_for > 0 {
        self.quiet_for -= 1;
        continue;
      }
      if score >= self.threshold && hit.is_none() {
        self.quiet_for = self.refractory_chunks;
        hit = Some(Detection {
          score,
          at_sample: self.consumed,
        });
      }
    }
    Ok(hit)
  }

  pub fn score(&self) -> Result<f32, Error> {
    let Some(tail) = self.features.tail(CLASSIFIER_FRAMES) else {
      return Ok(0.0);
    };
    let input =
      Tensor::from_shape(&[1, CLASSIFIER_FRAMES, EMBEDDING_DIM], tail).map_err(|e| Error::Infer(e.to_string()))?;
    let output = self
      .classifier
      .run(tvec!(input.into()))
      .map_err(|e| Error::Infer(e.to_string()))?;
    let scores = output[0]
      .view()
      .as_slice::<f32>()
      .map_err(|e| Error::Infer(e.to_string()))?;
    Ok(scores.first().copied().unwrap_or(0.0))
  }

  pub fn reset(&mut self) {
    self.features.reset();
    self.quiet_for = 0;
  }
}
