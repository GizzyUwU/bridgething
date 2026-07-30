use num_complex::Complex64;

use crate::geometry::{CHANNELS, diffuse_coherence, diffuse_covariance, steering_vector};

const MIN_LOADING: f64 = 1e-10;
const MAX_LOADING: f64 = 1e6;
const BISECTION_STEPS: usize = 48;

pub const MAX_NULLS: usize = 2;
const MAX_CONSTRAINTS: usize = MAX_NULLS + 1;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Nulls {
  angles: [f64; MAX_NULLS],
  count: usize,
}

impl Nulls {
  pub const NONE: Self = Self {
    angles: [0.0; MAX_NULLS],
    count: 0,
  };

  pub fn new(angles: &[f64]) -> Self {
    let count = angles.len().min(MAX_NULLS);
    let mut stored = [0.0; MAX_NULLS];
    stored[..count].copy_from_slice(&angles[..count]);
    Self { angles: stored, count }
  }

  pub fn angles(&self) -> &[f64] {
    &self.angles[..self.count]
  }

  pub fn is_empty(&self) -> bool {
    self.count == 0
  }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Design {
  DelayAndSum,
  Superdirective { wng_floor_db: f64 },
  Lcmv { wng_floor_db: f64, nulls: Nulls },
}

impl Design {
  pub const UNCONSTRAINED: Self = Self::Superdirective {
    wng_floor_db: f64::NEG_INFINITY,
  };

  fn wng_floor_db(&self) -> f64 {
    match self {
      Self::DelayAndSum => f64::NEG_INFINITY,
      Self::Superdirective { wng_floor_db } | Self::Lcmv { wng_floor_db, .. } => *wng_floor_db,
    }
  }

  fn nulls(&self) -> Nulls {
    match self {
      Self::Lcmv { nulls, .. } => *nulls,
      _ => Nulls::NONE,
    }
  }
}

pub type Covariance = [[Complex64; CHANNELS]; CHANNELS];

#[derive(Debug, Clone, Copy)]
pub struct Field {
  pub freq_hz: f64,
  pub steering: [Complex64; CHANNELS],
  pub noise: Covariance,
}

impl Field {
  pub fn assumed(freq_hz: f64, angle_deg: f64) -> Self {
    Self {
      freq_hz,
      steering: steering_vector(freq_hz, angle_deg),
      noise: diffuse_covariance(freq_hz),
    }
  }
}

fn cholesky(matrix: &Covariance) -> Option<Covariance> {
  let mut lower = [[Complex64::new(0.0, 0.0); CHANNELS]; CHANNELS];
  for i in 0..CHANNELS {
    for j in 0..=i {
      let dot: Complex64 = (0..j).map(|k| lower[i][k] * lower[j][k].conj()).sum();
      if i == j {
        let pivot = matrix[i][i].re - dot.re;
        if pivot <= 0.0 {
          return None;
        }
        lower[i][j] = Complex64::new(pivot.sqrt(), 0.0);
      } else {
        lower[i][j] = (matrix[i][j] - dot) / lower[j][j];
      }
    }
  }
  Some(lower)
}

fn solve_cholesky(lower: &Covariance, rhs: [Complex64; CHANNELS]) -> [Complex64; CHANNELS] {
  let mut y = [Complex64::new(0.0, 0.0); CHANNELS];
  for i in 0..CHANNELS {
    let dot: Complex64 = (0..i).map(|k| lower[i][k] * y[k]).sum();
    y[i] = (rhs[i] - dot) / lower[i][i];
  }
  let mut x = [Complex64::new(0.0, 0.0); CHANNELS];
  for i in (0..CHANNELS).rev() {
    let dot: Complex64 = (i + 1..CHANNELS).map(|k| lower[k][i].conj() * x[k]).sum();
    x[i] = (y[i] - dot) / lower[i][i];
  }
  x
}

fn solve_small_complex(
  mut matrix: [[Complex64; MAX_CONSTRAINTS]; MAX_CONSTRAINTS],
  mut rhs: [Complex64; MAX_CONSTRAINTS],
  size: usize,
) -> Option<[Complex64; MAX_CONSTRAINTS]> {
  for column in 0..size {
    let pivot = (column..size).max_by(|a, b| {
      matrix[*a][column]
        .norm()
        .partial_cmp(&matrix[*b][column].norm())
        .unwrap_or(std::cmp::Ordering::Equal)
    })?;
    if matrix[pivot][column].norm() < 1e-30 {
      return None;
    }
    matrix.swap(column, pivot);
    rhs.swap(column, pivot);

    let pivot_row = matrix[column];
    for row in column + 1..size {
      let factor = matrix[row][column] / matrix[column][column];
      for (k, value) in matrix[row][column..size].iter_mut().enumerate() {
        *value -= factor * pivot_row[column + k];
      }
      rhs[row] -= factor * rhs[column];
    }
  }

  let mut out = [Complex64::new(0.0, 0.0); MAX_CONSTRAINTS];
  for row in (0..size).rev() {
    let dot: Complex64 = (row + 1..size).map(|k| matrix[row][k] * out[k]).sum();
    out[row] = (rhs[row] - dot) / matrix[row][row];
  }
  Some(out)
}

fn lcmv(
  noise: &Covariance,
  constraints: &[[Complex64; CHANNELS]],
  responses: &[Complex64],
  loading: f64,
) -> Option<[Complex64; CHANNELS]> {
  let size = constraints.len();
  if size == 0 || size > MAX_CONSTRAINTS || size > CHANNELS {
    return None;
  }

  let mut loaded = *noise;
  for (i, row) in loaded.iter_mut().enumerate() {
    row[i] += loading;
  }
  let lower = cholesky(&loaded)?;

  let mut projected = [[Complex64::new(0.0, 0.0); CHANNELS]; MAX_CONSTRAINTS];
  for (index, constraint) in constraints.iter().enumerate() {
    projected[index] = solve_cholesky(&lower, *constraint);
  }

  let mut gram = [[Complex64::new(0.0, 0.0); MAX_CONSTRAINTS]; MAX_CONSTRAINTS];
  for row in 0..size {
    for column in 0..size {
      gram[row][column] = (0..CHANNELS)
        .map(|i| constraints[row][i].conj() * projected[column][i])
        .sum();
    }
  }

  let mut wanted = [Complex64::new(0.0, 0.0); MAX_CONSTRAINTS];
  wanted[..size].copy_from_slice(&responses[..size]);
  let coefficients = solve_small_complex(gram, wanted, size)?;

  let weights: [Complex64; CHANNELS] =
    std::array::from_fn(|i| (0..size).map(|j| projected[j][i] * coefficients[j]).sum());
  if weights.iter().any(|w| !w.re.is_finite() || !w.im.is_finite()) {
    return None;
  }
  Some(weights)
}

pub fn white_noise_gain_db(weights: &[Complex64; CHANNELS], steering: &[Complex64; CHANNELS]) -> f64 {
  let response: Complex64 = (0..CHANNELS).map(|i| weights[i].conj() * steering[i]).sum();
  let power: f64 = weights.iter().map(|w| w.norm_sqr()).sum();
  if power < 1e-30 {
    return f64::NEG_INFINITY;
  }
  10.0 * (response.norm_sqr() / power).log10()
}

pub fn directivity_index_db(weights: &[Complex64; CHANNELS], freq_hz: f64, angle_deg: f64) -> f64 {
  let steering = steering_vector(freq_hz, angle_deg);
  let coherence = diffuse_coherence(freq_hz);
  let response: Complex64 = (0..CHANNELS).map(|i| weights[i].conj() * steering[i]).sum();
  let mut noise = 0.0f64;
  for i in 0..CHANNELS {
    for j in 0..CHANNELS {
      noise += (weights[i].conj() * weights[j]).re * coherence[i][j];
    }
  }
  if noise < 1e-30 {
    return f64::INFINITY;
  }
  10.0 * (response.norm_sqr() / noise).log10()
}

fn constrained(field: &Field, nulls: Nulls, floor: f64) -> Option<[Complex64; CHANNELS]> {
  let mut constraints = [[Complex64::new(0.0, 0.0); CHANNELS]; MAX_CONSTRAINTS];
  let mut responses = [Complex64::new(0.0, 0.0); MAX_CONSTRAINTS];
  constraints[0] = field.steering;
  responses[0] = Complex64::new(1.0, 0.0);
  for (slot, null_deg) in nulls.angles().iter().enumerate() {
    constraints[slot + 1] = steering_vector(field.freq_hz, *null_deg);
  }
  let size = 1 + nulls.angles().len();
  let (constraints, responses) = (&constraints[..size], &responses[..size]);

  let lightest = lcmv(&field.noise, constraints, responses, MIN_LOADING)?;
  if !floor.is_finite() || white_noise_gain_db(&lightest, &field.steering) >= floor {
    return Some(lightest);
  }

  let (mut low, mut high) = (MIN_LOADING, MAX_LOADING);
  let mut best = None;
  for _ in 0..BISECTION_STEPS {
    let mid = (low.ln() + high.ln()).mul_add(0.5, 0.0).exp();
    match lcmv(&field.noise, constraints, responses, mid) {
      Some(candidate) if white_noise_gain_db(&candidate, &field.steering) >= floor => {
        best = Some(candidate);
        high = mid;
      }
      _ => low = mid,
    }
  }
  best
}

pub fn weights(design: Design, field: &Field) -> [Complex64; CHANNELS] {
  let delay_and_sum = || -> [Complex64; CHANNELS] { std::array::from_fn(|i| field.steering[i] / CHANNELS as f64) };
  if design == Design::DelayAndSum {
    return delay_and_sum();
  }

  let floor = design.wng_floor_db();
  let nulls = design.nulls();
  if let Some(weights) = constrained(field, nulls, floor) {
    return weights;
  }
  if !nulls.is_empty()
    && let Some(weights) = constrained(field, Nulls::NONE, floor)
  {
    return weights;
  }
  delay_and_sum()
}

pub fn response_db(weights: &[Complex64; CHANNELS], freq_hz: f64, source_deg: f64) -> f64 {
  let steering = steering_vector(freq_hz, source_deg);
  let response: Complex64 = (0..CHANNELS).map(|i| weights[i].conj() * steering[i]).sum();
  20.0 * response.norm().max(1e-30).log10()
}

#[cfg(test)]
mod tests {
  use super::*;

  const FLOOR: Design = Design::Superdirective { wng_floor_db: -10.0 };
  const BANDS: [f64; 8] = [200.0, 500.0, 750.0, 1000.0, 1500.0, 2000.0, 4000.0, 7900.0];

  fn di(design: Design, freq: f64, angle: f64) -> f64 {
    directivity_index_db(&weights(design, &Field::assumed(freq, angle)), freq, angle)
  }

  fn assert_row(design: Design, angle: f64, expected: [f64; 8]) {
    for (freq, want) in BANDS.iter().zip(expected) {
      let got = di(design, *freq, angle);
      assert!(
        (got - want).abs() < 0.05,
        "{angle} deg, {freq} Hz: expected {want:.2} dB, got {got:.2} dB"
      );
    }
  }

  #[test]
  fn delay_and_sum_at_broadside_matches_the_computed_table() {
    assert_row(
      Design::DelayAndSum,
      0.0,
      [0.01, 0.07, 0.16, 0.28, 0.63, 1.08, 3.41, 6.03],
    );
  }

  #[test]
  fn unconstrained_superdirective_at_broadside_matches_the_computed_table() {
    assert_row(
      Design::UNCONSTRAINED,
      0.0,
      [3.52, 3.53, 3.54, 3.56, 3.61, 3.69, 4.18, 6.03],
    );
  }

  #[test]
  fn constrained_superdirective_at_broadside_matches_the_computed_table() {
    assert_row(FLOOR, 0.0, [0.07, 0.43, 0.96, 1.68, 3.29, 3.69, 4.18, 6.03]);
  }

  #[test]
  fn delay_and_sum_off_broadside_matches_the_computed_table() {
    assert_row(
      Design::DelayAndSum,
      60.0,
      [0.04, 0.23, 0.51, 0.88, 1.82, 2.82, 5.06, 6.00],
    );
  }

  #[test]
  fn unconstrained_superdirective_off_broadside_matches_the_computed_table() {
    assert_row(
      Design::UNCONSTRAINED,
      60.0,
      [7.74, 7.74, 7.74, 7.74, 7.75, 7.75, 7.73, 6.00],
    );
  }

  #[test]
  fn unconstrained_directivity_converges_as_loading_vanishes() {
    let steering = steering_vector(200.0, 60.0);
    let coherence = diffuse_covariance(200.0);
    let mut previous = f64::NEG_INFINITY;
    for loading in [1e-6, 1e-8, 1e-10, 1e-12] {
      let di = directivity_index_db(
        &lcmv(&coherence, &[steering], &[Complex64::new(1.0, 0.0)], loading).unwrap(),
        200.0,
        60.0,
      );
      assert!(
        di >= previous - 1e-9,
        "directivity fell as loading dropped: {di:.4} after {previous:.4}"
      );
      previous = di;
    }
    assert!((previous - 7.739).abs() < 0.01);
  }

  #[test]
  fn constrained_superdirective_off_broadside_matches_the_computed_table() {
    assert_row(FLOOR, 60.0, [4.31, 5.45, 5.95, 6.55, 7.19, 7.38, 7.73, 6.00]);
  }

  #[test]
  fn constrained_superdirective_at_45_degrees_matches_the_computed_table() {
    for (freq, want) in [(500.0, 4.11), (750.0, 4.29), (1000.0, 4.47)] {
      let got = di(FLOOR, freq, 45.0);
      assert!(
        (got - want).abs() < 0.05,
        "{freq} Hz: expected {want:.2} dB, got {got:.2} dB"
      );
    }
  }

  #[test]
  fn steering_off_broadside_is_where_the_gain_lives() {
    assert!(di(FLOOR, 750.0, 60.0) > di(FLOOR, 750.0, 0.0) + 4.0);
  }

  #[test]
  fn delay_and_sum_white_noise_gain_is_exactly_the_element_count() {
    for freq in BANDS {
      for angle in [0.0, 35.0, 60.0] {
        let steering = steering_vector(freq, angle);
        let wng = white_noise_gain_db(&weights(Design::DelayAndSum, &Field::assumed(freq, angle)), &steering);
        assert!((wng - 10.0 * (CHANNELS as f64).log10()).abs() < 1e-9);
      }
    }
  }

  #[test]
  fn the_floor_binds_where_it_has_to_and_is_never_violated() {
    for freq in BANDS {
      for angle in [0.0, 35.0, 60.0] {
        let steering = steering_vector(freq, angle);
        let wng = white_noise_gain_db(&weights(FLOOR, &Field::assumed(freq, angle)), &steering);
        assert!(
          wng > -10.0 - 0.01,
          "{freq} Hz {angle} deg: wng {wng:.3} dB is below the floor"
        );
      }
    }
  }

  #[test]
  fn a_tighter_floor_costs_directivity_and_never_gains_it() {
    for freq in [500.0, 750.0, 1500.0] {
      let loose = di(Design::Superdirective { wng_floor_db: -10.0 }, freq, 60.0);
      let tight = di(Design::Superdirective { wng_floor_db: -3.0 }, freq, 60.0);
      assert!(tight <= loose + 1e-9, "{freq} Hz: tighter floor gave more directivity");
      assert!(tight >= di(Design::DelayAndSum, freq, 60.0) - 1e-9);
    }
  }

  #[test]
  fn weights_are_distortionless_on_the_look_direction() {
    for design in [Design::DelayAndSum, Design::UNCONSTRAINED, FLOOR] {
      for freq in BANDS {
        for angle in [0.0, 35.0, 60.0] {
          let steering = steering_vector(freq, angle);
          let w = weights(design, &Field::assumed(freq, angle));
          let response: Complex64 = (0..CHANNELS).map(|i| w[i].conj() * steering[i]).sum();
          assert!((response - Complex64::new(1.0, 0.0)).norm() < 1e-9);
        }
      }
    }
  }

  fn nulled(freq: f64, look: f64, interferers: &[f64]) -> [Complex64; CHANNELS] {
    weights(
      Design::Lcmv {
        wng_floor_db: -10.0,
        nulls: Nulls::new(interferers),
      },
      &Field::assumed(freq, look),
    )
  }

  #[test]
  fn lcmv_with_no_nulls_is_exactly_superdirective() {
    for freq in BANDS {
      for angle in [0.0, 35.0, 60.0] {
        let plain = weights(FLOOR, &Field::assumed(freq, angle));
        let empty = nulled(freq, angle, &[]);
        for (a, b) in plain.iter().zip(empty) {
          assert!((a - b).norm() < 1e-12, "{freq} Hz {angle} deg diverged");
        }
      }
    }
  }

  #[test]
  fn a_null_removes_the_interferer_and_leaves_the_target_alone() {
    for (look, interferer) in [(0.0, 60.0), (35.0, -60.0), (-35.0, 70.0)] {
      for freq in [750.0, 1500.0, 3000.0] {
        let w = nulled(freq, look, &[interferer]);
        assert!(
          response_db(&w, freq, interferer) < -60.0,
          "{freq} Hz: interferer at {interferer} only down {:.1} dB",
          response_db(&w, freq, interferer)
        );
        assert!(
          response_db(&w, freq, look).abs() < 1e-6,
          "{freq} Hz: look direction is not distortionless"
        );
      }
    }
  }

  #[test]
  fn two_nulls_are_unaffordable_in_the_speech_band_and_are_dropped() {
    for freq in [750.0, 1500.0] {
      let with_nulls = nulled(freq, 35.0, &[-70.0, 70.0]);
      let plain = weights(FLOOR, &Field::assumed(freq, 35.0));
      for (a, b) in with_nulls.iter().zip(plain) {
        assert!(
          (a - b).norm() < 1e-9,
          "{freq} Hz should have fallen back to superdirective"
        );
      }
    }
  }

  #[test]
  fn an_unaffordable_null_never_breaks_the_robustness_floor() {
    for freq in [200.0, 300.0, 500.0, 750.0, 1500.0, 4000.0] {
      for interferers in [vec![30.0], vec![-60.0], vec![-70.0, 70.0]] {
        let steering = steering_vector(freq, 35.0);
        let w = nulled(freq, 35.0, &interferers);
        let wng = white_noise_gain_db(&w, &steering);
        assert!(
          wng > -10.0 - 0.01,
          "{freq} Hz {interferers:?}: wng {wng:.2} dB broke the floor"
        );
        assert!(
          response_db(&w, freq, 35.0).abs() < 1e-6,
          "{freq} Hz: look direction is not distortionless"
        );
      }
    }
  }

  #[test]
  fn a_single_null_on_a_door_speaker_is_affordable_above_the_knee() {
    for freq in [750.0, 1500.0, 3000.0] {
      let w = nulled(freq, 35.0, &[-60.0]);
      assert!(
        response_db(&w, freq, -60.0) < -60.0,
        "{freq} Hz: the null was dropped, interferer only down {:.1} dB",
        response_db(&w, freq, -60.0)
      );
    }
  }

  #[test]
  fn nulls_still_respect_the_robustness_floor() {
    for freq in BANDS {
      let steering = steering_vector(freq, 35.0);
      let wng = white_noise_gain_db(&nulled(freq, 35.0, &[-60.0]), &steering);
      assert!(wng > -10.0 - 0.01, "{freq} Hz: wng {wng:.3} dB is below the floor");
    }
  }

  #[test]
  fn asking_for_more_nulls_than_the_array_can_hold_is_clamped() {
    let nulls = Nulls::new(&[-70.0, 70.0, 30.0, -30.0]);
    assert_eq!(nulls.angles().len(), MAX_NULLS);
    assert_eq!(nulls.angles(), &[-70.0, 70.0]);
  }

  #[test]
  fn at_the_aliasing_limit_every_design_collapses_to_delay_and_sum() {
    let freq = crate::geometry::spatial_aliasing_limit_hz();
    let das = weights(Design::DelayAndSum, &Field::assumed(freq, 35.0));
    for design in [Design::UNCONSTRAINED, FLOOR] {
      for (a, b) in weights(design, &Field::assumed(freq, 35.0)).iter().zip(das) {
        assert!((a - b).norm() < 1e-9);
      }
    }
  }

  fn scene_covariance(freq_hz: f64, interferers: &[(f64, f64)], diffuse_power: f64) -> Covariance {
    let gamma = diffuse_covariance(freq_hz);
    let mut noise: Covariance = std::array::from_fn(|i| std::array::from_fn(|j| gamma[i][j] * diffuse_power));
    for (angle, power) in interferers {
      let source = steering_vector(freq_hz, *angle);
      for i in 0..CHANNELS {
        for j in 0..CHANNELS {
          noise[i][j] += source[i] * source[j].conj() * *power;
        }
      }
    }
    noise
  }

  #[test]
  fn a_measured_covariance_nulls_an_interferer_nobody_named() {
    for freq in [750.0, 1500.0, 3000.0] {
      let field = Field {
        freq_hz: freq,
        steering: steering_vector(freq, 35.0),
        noise: scene_covariance(freq, &[(-60.0, 100.0)], 1.0),
      };
      let measured = weights(FLOOR, &field);
      let assumed = weights(FLOOR, &Field::assumed(freq, 35.0));

      let on_target = response_db(&measured, freq, 35.0);
      assert!(on_target.abs() < 1e-6, "{freq} Hz: distortionless constraint broken");

      let rejected = response_db(&measured, freq, -60.0);
      let unaware = response_db(&assumed, freq, -60.0);
      assert!(
        rejected < unaware - 10.0,
        "{freq} Hz: measured {rejected:.1} dB vs assumed {unaware:.1} dB at the interferer"
      );
    }
  }

  fn output_noise_power(weights: &[Complex64; CHANNELS], noise: &Covariance) -> f64 {
    (0..CHANNELS)
      .flat_map(|i| (0..CHANNELS).map(move |j| (i, j)))
      .map(|(i, j)| (weights[i].conj() * noise[i][j] * weights[j]).re)
      .sum()
  }

  #[test]
  fn a_measured_covariance_lowers_output_noise_power_on_the_scene_it_measured() {
    let scenes: [&[(f64, f64)]; 3] = [
      &[(-60.0, 100.0)],
      &[(-75.0, 40.0), (-40.0, 40.0), (-10.0, 40.0), (25.0, 40.0), (80.0, 40.0)],
      &[(-70.0, 60.0), (70.0, 60.0), (15.0, 20.0)],
    ];
    for interferers in scenes {
      for freq in [750.0, 1500.0, 3000.0] {
        let noise = scene_covariance(freq, interferers, 1.0);
        let measured = weights(
          FLOOR,
          &Field {
            freq_hz: freq,
            steering: steering_vector(freq, 35.0),
            noise,
          },
        );
        let assumed = weights(FLOOR, &Field::assumed(freq, 35.0));
        let (got, baseline) = (
          output_noise_power(&measured, &noise),
          output_noise_power(&assumed, &noise),
        );
        assert!(
          got <= baseline * 1.001,
          "{freq} Hz, {} sources: measured {got:.4} against assumed {baseline:.4}",
          interferers.len()
        );
      }
    }
  }

  #[test]
  fn only_a_measured_covariance_rejects_a_source_inside_the_mainlobe() {
    let freq = 1500.0;
    let noise = scene_covariance(freq, &[(25.0, 100.0)], 1.0);
    let rejection = |w: &[Complex64; CHANNELS]| -response_db(w, freq, 25.0);

    let measured = weights(
      FLOOR,
      &Field {
        freq_hz: freq,
        steering: steering_vector(freq, 35.0),
        noise,
      },
    );
    assert!(
      rejection(&measured) > 8.0,
      "measured only got {:.2} dB",
      rejection(&measured)
    );
    for blind in [
      weights(FLOOR, &Field::assumed(freq, 35.0)),
      weights(Design::DelayAndSum, &Field::assumed(freq, 35.0)),
    ] {
      assert!(
        rejection(&blind) < 3.0,
        "a design that cannot see the source rejected it by {:.2} dB",
        rejection(&blind)
      );
    }
  }

  #[test]
  fn an_isotropic_measurement_reproduces_the_assumed_design() {
    for freq in BANDS {
      for angle in [0.0, 35.0, -60.0] {
        let measured = weights(
          FLOOR,
          &Field {
            freq_hz: freq,
            steering: steering_vector(freq, angle),
            noise: diffuse_covariance(freq),
          },
        );
        for (a, b) in measured.iter().zip(weights(FLOOR, &Field::assumed(freq, angle))) {
          assert!(
            (a - b).norm() < 1e-12,
            "{freq} Hz {angle} deg diverged from the assumed design"
          );
        }
      }
    }
  }
}
