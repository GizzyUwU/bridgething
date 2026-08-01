use std::sync::Arc;

use realfft::{RealFftPlanner, RealToComplex};

pub const FFT_SIZE: usize = 512;
pub const WINDOW_SAMPLES: usize = 400;
pub const SPECTRUM_BINS: usize = FFT_SIZE / 2 + 1;
const WINDOW_OFFSET: usize = (FFT_SIZE - WINDOW_SAMPLES) / 2;
const MEL_FMIN: f32 = 60.0;
const MEL_FMAX: f32 = 3800.0;
const AMIN: f32 = 1e-10;
const TOP_DB: f32 = 80.0;
const INPUT_SCALE: f32 = 32767.0;

fn hz_to_mel(hz: f32) -> f32 {
  const LINEAR_HZ_PER_MEL: f32 = 200.0 / 3.0;
  const LOG_START_HZ: f32 = 1000.0;
  let log_start_mel = LOG_START_HZ / LINEAR_HZ_PER_MEL;
  if hz >= LOG_START_HZ {
    log_start_mel + (hz / LOG_START_HZ).ln() / (6.4f32.ln() / 27.0)
  } else {
    hz / LINEAR_HZ_PER_MEL
  }
}

fn mel_to_hz(mel: f32) -> f32 {
  const LINEAR_HZ_PER_MEL: f32 = 200.0 / 3.0;
  const LOG_START_HZ: f32 = 1000.0;
  let log_start_mel = LOG_START_HZ / LINEAR_HZ_PER_MEL;
  if mel >= log_start_mel {
    LOG_START_HZ * ((6.4f32.ln() / 27.0) * (mel - log_start_mel)).exp()
  } else {
    mel * LINEAR_HZ_PER_MEL
  }
}

struct Band {
  start: usize,
  weights: Vec<f32>,
}

fn filterbank(sample_rate: usize, bands: usize) -> Vec<Band> {
  let edges: Vec<f32> = {
    let (low, high) = (hz_to_mel(MEL_FMIN), hz_to_mel(MEL_FMAX));
    (0..bands + 2)
      .map(|i| mel_to_hz(low + (high - low) * i as f32 / (bands + 1) as f32))
      .collect()
  };
  let bin_hz = sample_rate as f32 / FFT_SIZE as f32;

  (0..bands)
    .map(|band| {
      let (left, centre, right) = (edges[band], edges[band + 1], edges[band + 2]);
      let norm = 2.0 / (right - left);
      let weight = |bin: usize| {
        let hz = bin as f32 * bin_hz;
        let rising = (hz - left) / (centre - left);
        let falling = (right - hz) / (right - centre);
        rising.min(falling).max(0.0) * norm
      };
      let start = (0..SPECTRUM_BINS).find(|&bin| weight(bin) > 0.0).unwrap_or(0);
      let end = (start..SPECTRUM_BINS).take_while(|&bin| weight(bin) > 0.0).count() + start;
      Band {
        start,
        weights: (start..end).map(weight).collect(),
      }
    })
    .collect()
}

fn hann_periodic() -> Vec<f32> {
  let mut window = vec![0.0; FFT_SIZE];
  for (i, slot) in window[WINDOW_OFFSET..WINDOW_OFFSET + WINDOW_SAMPLES]
    .iter_mut()
    .enumerate()
  {
    *slot = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / WINDOW_SAMPLES as f32).cos();
  }
  window
}

pub struct Melspectrogram {
  fft: Arc<dyn RealToComplex<f32>>,
  window: Vec<f32>,
  filters: Vec<Band>,
  used_bins: usize,
  hop: usize,
  frame: Vec<f32>,
  spectrum: Vec<realfft::num_complex::Complex<f32>>,
  power: Vec<f32>,
}

impl Melspectrogram {
  pub fn new(sample_rate: usize, bands: usize, hop: usize) -> Self {
    let fft = RealFftPlanner::<f32>::new().plan_fft_forward(FFT_SIZE);
    let filters = filterbank(sample_rate, bands);
    let used_bins = filters
      .iter()
      .map(|b| b.start + b.weights.len())
      .max()
      .unwrap_or(SPECTRUM_BINS);
    Self {
      window: hann_periodic(),
      filters,
      used_bins,
      hop,
      frame: vec![0.0; FFT_SIZE],
      spectrum: fft.make_output_vec(),
      power: vec![0.0; SPECTRUM_BINS],
      fft,
    }
  }

  pub fn frames_for(&self, samples: usize) -> usize {
    (samples.saturating_sub(FFT_SIZE)) / self.hop + usize::from(samples >= FFT_SIZE)
  }

  pub fn compute(&mut self, audio: &[f32], out: &mut Vec<f32>) {
    out.clear();
    for start in (0..).map(|f| f * self.hop).take(self.frames_for(audio.len())) {
      for ((slot, sample), window) in self
        .frame
        .iter_mut()
        .zip(&audio[start..start + FFT_SIZE])
        .zip(&self.window)
      {
        *slot = sample * INPUT_SCALE * window;
      }
      self
        .fft
        .process(&mut self.frame, &mut self.spectrum)
        .expect("fft buffers come from the same planner");

      for (slot, bin) in self.power[..self.used_bins].iter_mut().zip(&self.spectrum) {
        *slot = bin.re * bin.re + bin.im * bin.im;
      }
      for band in &self.filters {
        let power = &self.power[band.start..band.start + band.weights.len()];
        let energy: f32 = band.weights.iter().zip(power).map(|(w, p)| w * p).sum();
        out.push(10.0 * energy.max(AMIN).log10());
      }
    }

    let ceiling = out.iter().copied().fold(f32::NEG_INFINITY, f32::max) - TOP_DB;
    for value in out.iter_mut() {
      *value = value.max(ceiling);
    }
  }
}
