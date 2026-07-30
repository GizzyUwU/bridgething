pub const ARRAY_KNEE_HZ: f64 = 750.0;

#[derive(Debug, Clone, Copy)]
struct Biquad {
  b0: f32,
  b1: f32,
  b2: f32,
  a1: f32,
  a2: f32,
  s1: f32,
  s2: f32,
}

impl Biquad {
  fn high_pass(cutoff_hz: f64, sample_rate_hz: f64, q: f64) -> Self {
    let w0 = std::f64::consts::TAU * cutoff_hz / sample_rate_hz;
    let (sin, cos) = w0.sin_cos();
    let alpha = sin / (2.0 * q);
    let a0 = 1.0 + alpha;
    Self {
      b0: ((1.0 + cos) / 2.0 / a0) as f32,
      b1: (-(1.0 + cos) / a0) as f32,
      b2: ((1.0 + cos) / 2.0 / a0) as f32,
      a1: (-2.0 * cos / a0) as f32,
      a2: ((1.0 - alpha) / a0) as f32,
      s1: 0.0,
      s2: 0.0,
    }
  }

  fn process(&mut self, x: f32) -> f32 {
    let y = self.b0 * x + self.s1;
    self.s1 = self.b1 * x - self.a1 * y + self.s2;
    self.s2 = self.b2 * x - self.a2 * y;
    y
  }
}

#[derive(Debug, Clone)]
pub struct HighPass {
  sections: Vec<Biquad>,
}

impl HighPass {
  pub fn new(cutoff_hz: f64, sample_rate_hz: f64, order: usize) -> Self {
    assert!(
      order >= 2 && order.is_multiple_of(2),
      "order must be even and at least 2"
    );
    let sections = (0..order / 2)
      .map(|k| {
        let q = 1.0 / (2.0 * (std::f64::consts::PI * (2 * k + 1) as f64 / (2 * order) as f64).cos());
        Biquad::high_pass(cutoff_hz, sample_rate_hz, q)
      })
      .collect();
    Self { sections }
  }

  pub fn at_array_knee(sample_rate_hz: f64) -> Self {
    Self::new(ARRAY_KNEE_HZ, sample_rate_hz, 4)
  }

  pub fn process(&mut self, samples: &mut [f32]) {
    for sample in samples {
      let mut value = *sample;
      for section in &mut self.sections {
        value = section.process(value);
      }
      *sample = value;
    }
  }

  pub fn reset(&mut self) {
    for section in &mut self.sections {
      section.s1 = 0.0;
      section.s2 = 0.0;
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const RATE: f64 = 16_000.0;

  fn gain_db(filter: &mut HighPass, freq_hz: f64) -> f64 {
    filter.reset();
    let total = 8192;
    let mut signal: Vec<f32> = (0..total)
      .map(|n| (std::f64::consts::TAU * freq_hz * n as f64 / RATE).sin() as f32)
      .collect();
    filter.process(&mut signal);
    let settled = &signal[total / 2..];
    let rms = (settled.iter().map(|s| (s * s) as f64).sum::<f64>() / settled.len() as f64).sqrt();
    20.0 * (rms / (1.0 / 2f64.sqrt())).log10()
  }

  #[test]
  fn passband_is_flat_well_above_the_knee() {
    let mut filter = HighPass::at_array_knee(RATE);
    for freq in [2000.0, 3000.0, 4000.0] {
      assert!(gain_db(&mut filter, freq).abs() < 0.1, "{freq} Hz is not flat");
    }
  }

  #[test]
  fn the_cutoff_sits_three_db_down_at_the_array_knee() {
    let mut filter = HighPass::at_array_knee(RATE);
    let at_knee = gain_db(&mut filter, ARRAY_KNEE_HZ);
    assert!(
      (at_knee + 3.01).abs() < 0.2,
      "expected -3 dB at the knee, got {at_knee:.2}"
    );
  }

  #[test]
  fn fourth_order_rolls_off_at_twenty_four_db_per_octave() {
    let mut filter = HighPass::new(ARRAY_KNEE_HZ, RATE, 4);
    let (low, high) = (gain_db(&mut filter, 100.0), gain_db(&mut filter, 200.0));
    assert!((high - low - 24.0).abs() < 1.0, "octave slope was {:.1} dB", high - low);
  }

  #[test]
  fn second_order_rolls_off_at_twelve_db_per_octave() {
    let mut filter = HighPass::new(ARRAY_KNEE_HZ, RATE, 2);
    let (low, high) = (gain_db(&mut filter, 100.0), gain_db(&mut filter, 200.0));
    assert!((high - low - 12.0).abs() < 1.0, "octave slope was {:.1} dB", high - low);
  }

  #[test]
  fn road_rumble_well_below_the_knee_is_deeply_suppressed() {
    let mut filter = HighPass::at_array_knee(RATE);
    assert!(gain_db(&mut filter, 60.0) < -60.0);
  }

  #[test]
  fn reset_makes_the_filter_repeatable() {
    let mut filter = HighPass::at_array_knee(RATE);
    let first = gain_db(&mut filter, 1500.0);
    let second = gain_db(&mut filter, 1500.0);
    assert!((first - second).abs() < 1e-9);
  }
}
