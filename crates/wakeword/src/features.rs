use crate::{Error, embedding::Embedding, melspec::Melspectrogram};

pub const SAMPLE_RATE: usize = 16_000;
pub const CHUNK_SAMPLES: usize = 1280;
pub const MEL_BINS: usize = 32;
pub const MEL_HOP: usize = 160;
const MEL_CONTEXT_SAMPLES: usize = MEL_HOP * 3;
const MELSPEC_INPUT_SAMPLES: usize = CHUNK_SAMPLES + MEL_CONTEXT_SAMPLES;
pub const FRAMES_PER_CHUNK: usize = CHUNK_SAMPLES / MEL_HOP;
pub const FRAMES_PER_EMBEDDING: usize = 8;
pub const SAMPLES_PER_EMBEDDING: usize = FRAMES_PER_EMBEDDING * MEL_HOP;
pub const EMBEDDING_DIM: usize = 96;
const FEATURE_BUFFER_MAX: usize = 120;
const WARMUP_FRAMES: usize = 80;
const MEL_FILL: f32 = 1.0;

pub struct Features {
  melspectrogram: Melspectrogram,
  embedding: Embedding,
  frames_per_call: usize,
  audio: Vec<f32>,
  pending: Vec<f32>,
  mel: Vec<f32>,
  chunk_mel: Vec<f32>,
  embeddings: Vec<f32>,
}

const EMBEDDED_STACK: &[u8] = include_bytes!("../models/embedding_stream.btww");
const EMBEDDED_LABEL: &str = "<embedded embedding_stream.btww>";

impl Features {
  pub fn embedded() -> Result<Self, Error> {
    Self::build(Embedding::from_bytes(EMBEDDED_STACK, EMBEDDED_LABEL)?, EMBEDDED_LABEL)
  }

  pub fn new(embedding_path: &std::path::Path) -> Result<Self, Error> {
    let embedding = Embedding::load(embedding_path)?;
    let label = embedding_path.display().to_string();
    Self::build(embedding, &label)
  }

  fn build(embedding: Embedding, label: &str) -> Result<Self, Error> {
    let frames_per_call = embedding.frames_per_call();
    let fail = |reason: String| Error::Model(label.to_string(), reason);
    if frames_per_call != embedding.embeddings_per_call() * FRAMES_PER_EMBEDDING {
      return Err(fail(format!(
        "takes {frames_per_call} mel frames per call for {} embeddings, not one per {FRAMES_PER_EMBEDDING}",
        embedding.embeddings_per_call()
      )));
    }
    if embedding.embedding_dim() != EMBEDDING_DIM {
      return Err(fail(format!(
        "emits {}-wide embeddings, not {EMBEDDING_DIM}",
        embedding.embedding_dim()
      )));
    }

    let mut features = Self {
      melspectrogram: Melspectrogram::new(SAMPLE_RATE, MEL_BINS, MEL_HOP),
      embedding,
      frames_per_call,
      audio: Vec::with_capacity(MELSPEC_INPUT_SAMPLES),
      pending: Vec::with_capacity(CHUNK_SAMPLES),
      mel: Vec::with_capacity(frames_per_call * MEL_BINS),
      chunk_mel: Vec::with_capacity(FRAMES_PER_CHUNK * MEL_BINS),
      embeddings: Vec::with_capacity(FEATURE_BUFFER_MAX * EMBEDDING_DIM),
    };
    features.prime()?;
    Ok(features)
  }

  pub fn call_samples(&self) -> usize {
    self.frames_per_call * MEL_HOP
  }

  pub fn embedding_count(&self) -> usize {
    self.embeddings.len() / EMBEDDING_DIM
  }

  pub fn window(&self, frames: usize, back: usize) -> Option<&[f32]> {
    let end = self.embeddings.len().checked_sub(back * EMBEDDING_DIM)?;
    let start = end.checked_sub(frames * EMBEDDING_DIM)?;
    Some(&self.embeddings[start..end])
  }

  pub fn tail(&self, frames: usize) -> Option<&[f32]> {
    self.window(frames, 0)
  }

  pub fn reset(&mut self) -> Result<(), Error> {
    self.audio.clear();
    self.pending.clear();
    self.mel.clear();
    self.embeddings.clear();
    self.embedding.reset();
    self.prime()
  }

  pub fn push(&mut self, samples: &[f32]) -> Result<usize, Error> {
    self.pending.extend_from_slice(samples);
    let mut produced = 0;
    while self.pending.len() >= CHUNK_SAMPLES {
      let chunk: Vec<f32> = self.pending.drain(..CHUNK_SAMPLES).collect();
      self.audio.extend_from_slice(&chunk);
      if self.audio.len() > MELSPEC_INPUT_SAMPLES {
        let excess = self.audio.len() - MELSPEC_INPUT_SAMPLES;
        self.audio.drain(..excess);
      }
      if self.audio.len() == MELSPEC_INPUT_SAMPLES {
        produced += self.advance()?;
      }
    }
    Ok(produced)
  }

  fn prime(&mut self) -> Result<(), Error> {
    self.mel.clear();
    self.mel.resize(self.frames_per_call * MEL_BINS, MEL_FILL);
    for _ in 0..WARMUP_FRAMES.div_ceil(self.frames_per_call) {
      self.embedding.run(&self.mel)?;
    }
    self.mel.clear();
    Ok(())
  }

  fn advance(&mut self) -> Result<usize, Error> {
    self.melspectrogram.compute(&self.audio, &mut self.chunk_mel);
    debug_assert_eq!(self.chunk_mel.len(), FRAMES_PER_CHUNK * MEL_BINS);
    self.mel.extend(self.chunk_mel.iter().map(|value| value / 10.0 + 2.0));
    if self.mel.len() < self.frames_per_call * MEL_BINS {
      return Ok(0);
    }

    let produced = self.embedding.run(&self.mel)?;
    self.embeddings.extend_from_slice(produced);
    self.mel.clear();
    let max = FEATURE_BUFFER_MAX * EMBEDDING_DIM;
    if self.embeddings.len() > max {
      self.embeddings.drain(..self.embeddings.len() - max);
    }
    Ok(self.embedding.embeddings_per_call())
  }
}
