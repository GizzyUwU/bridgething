use num_complex::Complex64;
use serde::Deserialize;

use crate::geometry::CHANNELS;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Calibration {
  gain_db: [f64; CHANNELS],
  delay_samples: [f64; CHANNELS],
}

#[derive(Debug, Clone, Deserialize)]
pub struct Measurement {
  pub gain_db: [f64; CHANNELS],
  #[serde(default)]
  pub residual_complex_db_rms: f64,
}

impl Default for Calibration {
  fn default() -> Self {
    Self::IDENTITY
  }
}

impl Calibration {
  pub const IDENTITY: Self = Self {
    gain_db: [0.0; CHANNELS],
    delay_samples: [0.0; CHANNELS],
  };

  pub fn new(gain_db: [f64; CHANNELS], delay_samples: [f64; CHANNELS]) -> Self {
    Self { gain_db, delay_samples }
  }

  pub fn gains(&self) -> [f32; CHANNELS] {
    std::array::from_fn(|c| 10f64.powf(self.gain_db[c] / 20.0) as f32)
  }

  pub fn bin_correction(&self, bin_freq_hz: f64, sample_rate_hz: f64) -> [Complex64; CHANNELS] {
    std::array::from_fn(|c| {
      let phase = std::f64::consts::TAU * bin_freq_hz * self.delay_samples[c] / sample_rate_hz;
      Complex64::from_polar(1.0, phase)
    })
  }

  pub fn is_identity(&self) -> bool {
    *self == Self::IDENTITY
  }
}

impl From<Measurement> for Calibration {
  fn from(measurement: Measurement) -> Self {
    Self {
      gain_db: std::array::from_fn(|c| -measurement.gain_db[c]),
      delay_samples: [0.0; CHANNELS],
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn identity_leaves_every_channel_alone() {
    for gain in Calibration::IDENTITY.gains() {
      assert!((gain - 1.0).abs() < 1e-12);
    }
  }

  #[test]
  fn a_measurement_becomes_the_correction_that_cancels_it() {
    let measured = Measurement {
      gain_db: [-0.01, -0.57, -0.24, 0.70],
      residual_complex_db_rms: 3.29,
    };
    let gains = Calibration::from(measured.clone()).gains();
    for (c, gain) in gains.iter().enumerate().take(CHANNELS) {
      let corrected_db = measured.gain_db[c] + 20.0 * (*gain as f64).log10();
      assert!(corrected_db.abs() < 1e-5, "ch{c} still off by {corrected_db} dB");
    }
    assert!(gains[3] < 1.0 && gains[1] > 1.0);
  }

  #[test]
  fn a_delay_correction_is_a_pure_phase_ramp() {
    let calibration = Calibration::new([0.0; CHANNELS], [0.0, 0.5, -0.5, 0.0]);
    for bin_freq in [250.0, 1000.0, 4000.0] {
      let correction = calibration.bin_correction(bin_freq, 16_000.0);
      for channel in correction {
        assert!((channel.norm() - 1.0).abs() < 1e-12);
      }
      assert!((correction[1] * correction[2] - Complex64::new(1.0, 0.0)).norm() < 1e-12);
    }
  }

  #[test]
  fn measurement_parses_from_what_the_python_rig_writes() {
    let json = r#"{
      "reference_channel": 0,
      "gain_db": [-0.0104, -0.5721, -0.2384, 0.6968],
      "gain_spread_db": [0.06, 0.31, 0.15, 0.06],
      "residual_magnitude_db_rms": 3.11,
      "residual_complex_db_rms": 3.29,
      "takes": [{"wav": "takes/take-front45.wav", "angle_deg": 45.0}]
    }"#;
    let measurement: Measurement = serde_json::from_str(json).expect("rig output should parse");
    assert!((measurement.gain_db[3] - 0.6968).abs() < 1e-9);
    assert!((measurement.residual_complex_db_rms - 3.29).abs() < 1e-9);
  }
}
