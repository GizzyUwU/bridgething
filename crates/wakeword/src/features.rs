use tract_onnx::prelude::*;

use crate::Error;

pub const SAMPLE_RATE: usize = 16_000;
pub const CHUNK_SAMPLES: usize = 1280;
pub const MEL_BINS: usize = 32;
pub const MEL_HOP: usize = 160;
const MEL_CONTEXT_SAMPLES: usize = MEL_HOP * 3;
const MELSPEC_INPUT_SAMPLES: usize = CHUNK_SAMPLES + MEL_CONTEXT_SAMPLES;
const FRAMES_PER_CHUNK: usize = CHUNK_SAMPLES / MEL_HOP;
pub const EMBEDDING_WINDOW: usize = 76;
pub const EMBEDDING_DIM: usize = 96;
const MEL_BUFFER_MAX: usize = 970;
const FEATURE_BUFFER_MAX: usize = 120;

type Runnable = std::sync::Arc<TypedRunnableModel>;

fn load(path: &std::path::Path, shape: [usize; 4], rank: usize) -> Result<Runnable, Error> {
  let fact = match rank {
    2 => f32::fact([shape[0], shape[1]]).into(),
    _ => f32::fact(shape).into(),
  };

  tract_onnx::onnx()
    .model_for_path(path)
    .map_err(|e| Error::Model(path.display().to_string(), e.to_string()))?
    .with_input_fact(0, fact)
    .map_err(|e| Error::Model(path.display().to_string(), e.to_string()))?
    .into_optimized()
    .map_err(|e| Error::Model(path.display().to_string(), e.to_string()))?
    .into_runnable()
    .map_err(|e| Error::Model(path.display().to_string(), e.to_string()))
}

pub struct Features {
  melspectrogram: Runnable,
  embedding: Runnable,
  audio: Vec<f32>,
  pending: Vec<f32>,
  mel: Vec<f32>,
  embeddings: Vec<f32>,
}

impl Features {
  pub fn new(melspectrogram_path: &std::path::Path, embedding_path: &std::path::Path) -> Result<Self, Error> {
    Ok(Self {
      melspectrogram: load(melspectrogram_path, [1, MELSPEC_INPUT_SAMPLES, 0, 0], 2)?,
      embedding: load(embedding_path, [1, EMBEDDING_WINDOW, MEL_BINS, 1], 4)?,
      audio: Vec::with_capacity(MELSPEC_INPUT_SAMPLES * 2),
      pending: Vec::with_capacity(CHUNK_SAMPLES),
      mel: vec![1.0; EMBEDDING_WINDOW * MEL_BINS],
      embeddings: Vec::with_capacity(FEATURE_BUFFER_MAX * EMBEDDING_DIM),
    })
  }

  pub fn embedding_count(&self) -> usize {
    self.embeddings.len() / EMBEDDING_DIM
  }

  pub fn tail(&self, frames: usize) -> Option<&[f32]> {
    let needed = frames * EMBEDDING_DIM;
    self
      .embeddings
      .len()
      .checked_sub(needed)
      .map(|start| &self.embeddings[start..])
  }

  pub fn reset(&mut self) {
    self.audio.clear();
    self.pending.clear();
    self.mel.clear();
    self.mel.resize(EMBEDDING_WINDOW * MEL_BINS, 1.0);
    self.embeddings.clear();
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
        self.advance()?;
        produced += 1;
      }
    }
    Ok(produced)
  }

  fn advance(&mut self) -> Result<(), Error> {
    let scaled: Vec<f32> = self.audio.iter().map(|s| s * 32767.0).collect();
    let input = Tensor::from_shape(&[1, MELSPEC_INPUT_SAMPLES], &scaled).map_err(|e| Error::Infer(e.to_string()))?;
    let output = self
      .melspectrogram
      .run(tvec!(input.into()))
      .map_err(|e| Error::Infer(e.to_string()))?;
    let frames = output[0]
      .view()
      .as_slice::<f32>()
      .map_err(|e| Error::Infer(e.to_string()))?;

    self.mel.extend(frames.iter().map(|v| v / 10.0 + 2.0));
    debug_assert_eq!(frames.len(), FRAMES_PER_CHUNK * MEL_BINS);
    let max = MEL_BUFFER_MAX * MEL_BINS;
    if self.mel.len() > max {
      self.mel.drain(..self.mel.len() - max);
    }

    let window_start = self.mel.len() - EMBEDDING_WINDOW * MEL_BINS;
    let window = Tensor::from_shape(&[1, EMBEDDING_WINDOW, MEL_BINS, 1], &self.mel[window_start..])
      .map_err(|e| Error::Infer(e.to_string()))?;
    let embedded = self
      .embedding
      .run(tvec!(window.into()))
      .map_err(|e| Error::Infer(e.to_string()))?;
    let vector = embedded[0]
      .view()
      .as_slice::<f32>()
      .map_err(|e| Error::Infer(e.to_string()))?;

    self.embeddings.extend_from_slice(vector);
    let max = FEATURE_BUFFER_MAX * EMBEDDING_DIM;
    if self.embeddings.len() > max {
      self.embeddings.drain(..self.embeddings.len() - max);
    }
    Ok(())
  }
}
