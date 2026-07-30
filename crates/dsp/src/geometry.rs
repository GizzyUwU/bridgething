use num_complex::Complex64;

pub const CHANNELS: usize = 4;
pub const ELEMENT_SPACING_M: f64 = 0.021_75;
pub const SPEED_OF_SOUND_M_S: f64 = 343.0;
pub const APERTURE_M: f64 = ELEMENT_SPACING_M * (CHANNELS - 1) as f64;
pub const POSITION_TO_CHANNEL: [usize; CHANNELS] = [1, 0, 3, 2];

pub const fn spatial_aliasing_limit_hz() -> f64 {
  SPEED_OF_SOUND_M_S / (2.0 * ELEMENT_SPACING_M)
}

pub fn position_offsets_m() -> [f64; CHANNELS] {
  let centre = (CHANNELS - 1) as f64 / 2.0;
  std::array::from_fn(|p| (p as f64 - centre) * ELEMENT_SPACING_M)
}

pub fn steering_vector(freq_hz: f64, angle_deg: f64) -> [Complex64; CHANNELS] {
  let offsets = position_offsets_m();
  let sin_theta = angle_deg.to_radians().sin();
  std::array::from_fn(|p| {
    let phase = std::f64::consts::TAU * freq_hz * offsets[p] * sin_theta / SPEED_OF_SOUND_M_S;
    Complex64::from_polar(1.0, phase)
  })
}

pub fn diffuse_covariance(freq_hz: f64) -> [[Complex64; CHANNELS]; CHANNELS] {
  let gamma = diffuse_coherence(freq_hz);
  std::array::from_fn(|i| std::array::from_fn(|j| Complex64::new(gamma[i][j], 0.0)))
}

pub fn diffuse_coherence(freq_hz: f64) -> [[f64; CHANNELS]; CHANNELS] {
  let offsets = position_offsets_m();
  let wavenumber = std::f64::consts::TAU * freq_hz / SPEED_OF_SOUND_M_S;
  std::array::from_fn(|i| {
    std::array::from_fn(|j| {
      let z = wavenumber * (offsets[i] - offsets[j]).abs();
      if z.abs() < 1e-12 { 1.0 } else { z.sin() / z }
    })
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn aliasing_limit_sits_on_the_nyquist_of_a_16k_pipeline() {
    assert!((spatial_aliasing_limit_hz() - 7885.0).abs() < 1.0);
  }

  #[test]
  fn aperture_matches_the_caliper_measurement() {
    assert!((APERTURE_M - 0.065_25).abs() < 1e-9);
  }

  #[test]
  fn positions_are_centred_and_evenly_spaced() {
    let offsets = position_offsets_m();
    assert!((offsets.iter().sum::<f64>()).abs() < 1e-12);
    for pair in offsets.windows(2) {
      assert!((pair[1] - pair[0] - ELEMENT_SPACING_M).abs() < 1e-12);
    }
  }

  #[test]
  fn permutation_is_a_bijection_over_the_wire_channels() {
    let mut seen = POSITION_TO_CHANNEL;
    seen.sort_unstable();
    assert_eq!(seen, [0, 1, 2, 3]);
  }

  #[test]
  fn broadside_steering_vector_is_all_ones() {
    for element in steering_vector(1000.0, 0.0) {
      assert!((element - Complex64::new(1.0, 0.0)).norm() < 1e-12);
    }
  }

  #[test]
  fn steering_phase_reverses_with_the_sign_of_the_angle() {
    let right = steering_vector(1000.0, 35.0);
    let left = steering_vector(1000.0, -35.0);
    for p in 0..CHANNELS {
      assert!((right[p] - left[p].conj()).norm() < 1e-12);
    }
  }

  #[test]
  fn endfire_delay_across_one_element_is_just_over_one_sample_at_16k() {
    let samples = ELEMENT_SPACING_M / SPEED_OF_SOUND_M_S * 16_000.0;
    assert!((samples - 1.0146).abs() < 1e-3);
  }

  #[test]
  fn diffuse_coherence_is_symmetric_with_a_unit_diagonal() {
    let gamma = diffuse_coherence(750.0);
    for (i, row) in gamma.iter().enumerate() {
      assert!((row[i] - 1.0).abs() < 1e-12);
      for (j, value) in row.iter().enumerate() {
        assert!((*value - gamma[j][i]).abs() < 1e-12);
      }
    }
  }

  #[test]
  fn diffuse_coherence_first_null_lands_on_the_widest_pair() {
    let null_hz = SPEED_OF_SOUND_M_S / (2.0 * APERTURE_M);
    let gamma = diffuse_coherence(null_hz);
    assert!(gamma[0][CHANNELS - 1].abs() < 1e-9);
    assert!((null_hz - 2628.0).abs() < 2.0);
  }
}
