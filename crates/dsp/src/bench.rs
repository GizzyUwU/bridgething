use std::path::Path;

use serde::Deserialize;

use crate::geometry::CHANNELS;

#[derive(Deserialize)]
pub struct Scene {
  pub audio: String,
  pub archetype: String,
  #[serde(default)]
  pub talker_deg: f64,
  #[serde(default)]
  pub n_speakers: usize,
  pub snr_db: Option<f64>,
  pub speech_start: usize,
  pub speech_len: usize,
  #[serde(default)]
  pub speech_spans: Vec<[usize; 2]>,
}

impl Scene {
  pub fn has_speech(&self) -> bool {
    self.speech_len > 0
  }

  pub fn spans(&self) -> Vec<(usize, usize)> {
    if self.speech_spans.is_empty() {
      if self.speech_len == 0 {
        Vec::new()
      } else {
        vec![(self.speech_start, self.speech_len)]
      }
    } else {
      self.speech_spans.iter().map(|span| (span[0], span[1])).collect()
    }
  }
}

pub fn read_scene(path: &Path) -> Option<Vec<i32>> {
  let mut reader = hound::WavReader::open(path).ok()?;
  let spec = reader.spec();
  assert_eq!(spec.channels as usize, CHANNELS, "scene {path:?} is not 4-channel");
  let samples: Vec<i32> = match spec.sample_format {
    hound::SampleFormat::Float => reader
      .samples::<f32>()
      .filter_map(Result::ok)
      .map(|s| (s.clamp(-1.0, 1.0) as f64 * i32::MAX as f64) as i32)
      .collect(),
    hound::SampleFormat::Int => reader
      .samples::<i32>()
      .filter_map(Result::ok)
      .map(|s| s << (32 - spec.bits_per_sample))
      .collect(),
  };
  Some(samples)
}
