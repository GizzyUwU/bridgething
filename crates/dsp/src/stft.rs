use std::sync::Arc;

use num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

pub const FFT_SIZE: usize = 256;
pub const HOP: usize = FFT_SIZE / 2;
pub const BINS: usize = FFT_SIZE / 2 + 1;

fn root_hann() -> Vec<f32> {
  (0..FFT_SIZE)
    .map(|n| {
      let hann = 0.5 * (1.0 - (std::f64::consts::TAU * n as f64 / FFT_SIZE as f64).cos());
      hann.sqrt() as f32
    })
    .collect()
}

pub fn bin_frequencies(sample_rate_hz: f64) -> [f64; BINS] {
  std::array::from_fn(|k| k as f64 * sample_rate_hz / FFT_SIZE as f64)
}

pub struct Analyzer {
  forward: Arc<dyn RealToComplex<f32>>,
  window: Vec<f32>,
  scratch: Vec<f32>,
}

impl Analyzer {
  pub fn new(planner: &mut RealFftPlanner<f32>) -> Self {
    Self {
      forward: planner.plan_fft_forward(FFT_SIZE),
      window: root_hann(),
      scratch: vec![0.0; FFT_SIZE],
    }
  }

  pub fn analyze(&mut self, frame: &[f32], spectrum: &mut [Complex32]) {
    for (dst, (sample, window)) in self.scratch.iter_mut().zip(frame.iter().zip(&self.window)) {
      *dst = sample * window;
    }
    self
      .forward
      .process(&mut self.scratch, spectrum)
      .expect("fft buffers are sized from the same constants");
  }
}

pub struct Synthesizer {
  inverse: Arc<dyn ComplexToReal<f32>>,
  window: Vec<f32>,
  scratch: Vec<f32>,
  overlap: Vec<f32>,
}

impl Synthesizer {
  pub fn new(planner: &mut RealFftPlanner<f32>) -> Self {
    Self {
      inverse: planner.plan_fft_inverse(FFT_SIZE),
      window: root_hann(),
      scratch: vec![0.0; FFT_SIZE],
      overlap: vec![0.0; FFT_SIZE],
    }
  }

  pub fn synthesize(&mut self, spectrum: &mut [Complex32], out: &mut [f32]) {
    spectrum[0].im = 0.0;
    spectrum[BINS - 1].im = 0.0;

    self
      .inverse
      .process(spectrum, &mut self.scratch)
      .expect("fft buffers are sized from the same constants");

    let normalize = 1.0 / FFT_SIZE as f32;
    for ((tail, sample), window) in self.overlap.iter_mut().zip(&self.scratch).zip(&self.window) {
      *tail += sample * normalize * window;
    }

    out.copy_from_slice(&self.overlap[..HOP]);
    self.overlap.copy_within(HOP.., 0);
    self.overlap[FFT_SIZE - HOP..].fill(0.0);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn pseudo_noise(len: usize) -> Vec<f32> {
    (0..len)
      .map(|n| {
        (1..=9)
          .map(|h| ((n as f64 * 0.017 * h as f64 + h as f64).sin() / h as f64) as f32)
          .sum::<f32>()
      })
      .collect()
  }

  #[test]
  fn root_hann_squares_to_a_window_that_sums_to_unity_at_fifty_percent_overlap() {
    let window = root_hann();
    for n in 0..HOP {
      let sum = window[n] * window[n] + window[n + HOP] * window[n + HOP];
      assert!((sum - 1.0).abs() < 1e-6, "overlap at {n} sums to {sum}");
    }
  }

  #[test]
  fn analysis_then_synthesis_reconstructs_the_input_exactly() {
    let mut planner = RealFftPlanner::<f32>::new();
    let (mut analyzer, mut synthesizer) = (Analyzer::new(&mut planner), Synthesizer::new(&mut planner));
    let input = pseudo_noise(FFT_SIZE * 8);

    let mut spectrum = vec![Complex32::new(0.0, 0.0); BINS];
    let mut output = Vec::new();
    let mut hop = vec![0.0f32; HOP];
    for start in (0..input.len() - FFT_SIZE).step_by(HOP) {
      analyzer.analyze(&input[start..start + FFT_SIZE], &mut spectrum);
      synthesizer.synthesize(&mut spectrum, &mut hop);
      output.extend_from_slice(&hop);
    }

    for n in HOP..output.len() {
      assert!(
        (output[n] - input[n]).abs() < 1e-4,
        "sample {n}: {} vs {}",
        output[n],
        input[n]
      );
    }
  }

  #[test]
  fn bin_frequencies_span_dc_to_nyquist() {
    let freqs = bin_frequencies(16_000.0);
    assert!(freqs[0].abs() < 1e-12);
    assert!((freqs[BINS - 1] - 8000.0).abs() < 1e-9);
    assert!((freqs[1] - 62.5).abs() < 1e-9);
  }
}
