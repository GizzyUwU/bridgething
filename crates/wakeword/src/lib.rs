pub mod blob;
pub mod classifier;
pub mod embedding;
pub mod features;
pub mod lane;
pub mod melspec;

use std::path::Path;

use classifier::Classifier;
use features::{EMBEDDING_DIM, Features, SAMPLES_PER_EMBEDDING};

#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error("loading {0}: {1}")]
  Model(String, String),
  #[error("inference: {0}")]
  Infer(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Detection {
  pub score: f32,
  pub at_sample: u64,
}

pub struct WakeWord {
  features: Features,
  classifier: Classifier,
  window: Vec<f32>,
  scores: Vec<f32>,
  threshold: f32,
  refractory_chunks: usize,
  quiet_for: usize,
  consumed: u64,
}

impl WakeWord {
  pub fn new(phrase_model: &Path, threshold: f32) -> Result<Self, Error> {
    let features = Features::embedded()?;
    let classifier = Classifier::load(phrase_model)?;
    if classifier.embedding_dim() != EMBEDDING_DIM {
      return Err(Error::Model(
        phrase_model.display().to_string(),
        format!(
          "takes {}-wide embeddings, and the stack emits {EMBEDDING_DIM}",
          classifier.embedding_dim()
        ),
      ));
    }

    Ok(Self {
      features,
      classifier,
      window: Vec::new(),
      scores: Vec::new(),
      threshold,
      refractory_chunks: 25,
      quiet_for: 0,
      consumed: 0,
    })
  }

  pub fn push(&mut self, samples: &[f32]) -> Result<Option<Detection>, Error> {
    let mut hit = None;
    for slice in samples.chunks(self.features.call_samples()) {
      let produced = self.features.push(slice)?;
      self.consumed += slice.len() as u64;
      let found = self.detect(produced)?;
      hit = hit.or(found);
    }
    Ok(hit)
  }

  fn detect(&mut self, produced: usize) -> Result<Option<Detection>, Error> {
    let backs: Vec<usize> = (0..produced).rev().collect();
    let scores = self.score_windows(&backs)?;

    let mut hit = None;
    for (&back, &score) in backs.iter().zip(&scores) {
      if self.quiet_for > 0 {
        self.quiet_for -= 1;
        continue;
      }
      if score >= self.threshold && hit.is_none() {
        self.quiet_for = self.refractory_chunks;
        hit = Some(Detection {
          score,
          at_sample: self.consumed - (back * SAMPLES_PER_EMBEDDING) as u64,
        });
      }
    }
    Ok(hit)
  }

  pub fn score(&mut self) -> Result<f32, Error> {
    self.score_at(0)
  }

  pub fn embedding_count(&self) -> usize {
    self.features.embedding_count()
  }

  pub fn score_at(&mut self, back: usize) -> Result<f32, Error> {
    Ok(self.score_windows(&[back])?[0])
  }

  fn score_windows(&mut self, backs: &[usize]) -> Result<Vec<f32>, Error> {
    let frames = self.classifier.window_frames();
    self.window.clear();
    let mut present = 0;
    for &back in backs {
      if let Some(window) = self.features.window(frames, back) {
        self.window.extend_from_slice(window);
        present += 1;
      }
    }

    self.scores.clear();
    self.scores.resize(backs.len() - present, 0.0);
    if present > 0 {
      self.classifier.score(&self.window, present, &mut self.scores)?;
    }
    Ok(self.scores.clone())
  }

  pub fn reset(&mut self) -> Result<(), Error> {
    self.quiet_for = 0;
    self.features.reset()
  }
}
