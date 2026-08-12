use std::{collections::VecDeque, sync::Arc, time::Duration};

use bridgething_sdk_runtime::rt::Instant;

use crate::seam::Clock;

pub const TARGET_DELAY: Duration = Duration::from_millis(600);
pub const ACK_INTERVAL_BYTES: u64 = 16 * 1024;
pub const MIN_WINDOW_BYTES: u64 = 4 * ACK_INTERVAL_BYTES;
pub const MAX_WINDOW_BYTES: u64 = 16 * ACK_INTERVAL_BYTES;
pub const FRAGMENT_BYTES: usize = 16 * 1024;
pub const RATE_SAMPLES: usize = 8;
const MIN_SAMPLE_SECONDS: f64 = 0.001;

pub struct Pacer {
  clock: Arc<dyn Clock>,
  acked: u64,
  last_progress: Instant,
  samples: VecDeque<f64>,
}

impl Pacer {
  pub fn new(clock: Arc<dyn Clock>, start_offset: u64) -> Self {
    let last_progress = clock.now();
    Self {
      clock,
      acked: start_offset,
      last_progress,
      samples: VecDeque::with_capacity(RATE_SAMPLES),
    }
  }

  pub fn observe(&mut self, acked: u64) {
    if acked <= self.acked {
      return;
    }
    let now = self.clock.now();
    let elapsed = (now - self.last_progress).as_secs_f64().max(MIN_SAMPLE_SECONDS);
    if self.samples.len() == RATE_SAMPLES {
      self.samples.pop_front();
    }
    self.samples.push_back((acked - self.acked) as f64 / elapsed);
    self.acked = acked;
    self.last_progress = now;
  }

  pub fn acked(&self) -> u64 {
    self.acked
  }

  pub fn rate_per_sec(&self) -> Option<f64> {
    self
      .samples
      .iter()
      .copied()
      .fold(None::<f64>, |best, sample| Some(best.map_or(sample, |b| b.max(sample))))
  }

  pub fn window_bytes(&self) -> u64 {
    let Some(rate) = self.rate_per_sec() else {
      return MIN_WINDOW_BYTES;
    };
    ((rate * TARGET_DELAY.as_secs_f64()).round() as u64).clamp(MIN_WINDOW_BYTES, MAX_WINDOW_BYTES)
  }
}

#[cfg(test)]
mod tests {
  use super::{super::fixture::TestClock, *};

  fn pacer_at(start_offset: u64) -> (Pacer, Arc<TestClock>) {
    let clock = TestClock::new();
    (Pacer::new(clock.clone(), start_offset), clock)
  }

  fn simulate(link_bytes_per_sec: f64, rtt_seconds: f64, seconds: f64) -> (f64, u64) {
    let (mut pacer, clock) = pacer_at(0);
    let mut acked = 0u64;
    let mut elapsed = 0.0;
    while elapsed < seconds {
      let batch = pacer.window_bytes();
      let on_wire = batch as f64 / link_bytes_per_sec;
      let step = on_wire.max(rtt_seconds);
      clock.advance_secs(step);
      elapsed += step;
      acked += batch;
      pacer.observe(acked);
    }
    (acked as f64 / elapsed, pacer.window_bytes())
  }

  #[test]
  fn an_unmeasured_pacer_opens_at_the_floor() {
    let (pacer, _clock) = pacer_at(0);
    assert_eq!(pacer.window_bytes(), MIN_WINDOW_BYTES);
    assert_eq!(pacer.rate_per_sec(), None);
  }

  #[test]
  fn the_floor_spans_several_fragments_so_the_stream_never_stops_and_waits() {
    let (pacer, _clock) = pacer_at(0);
    assert!(pacer.window_bytes() >= 4 * ACK_INTERVAL_BYTES);
    assert!(
      pacer.window_bytes() / FRAGMENT_BYTES as u64 >= 4,
      "at least four fragments must be in flight before the first ack is needed"
    );
  }

  #[test]
  fn a_slow_link_stays_at_the_floor() {
    let (mut pacer, clock) = pacer_at(0);
    for i in 1..=4u64 {
      clock.advance(Duration::from_secs(1));
      pacer.observe(i * 16 * 1024);
    }
    assert_eq!(pacer.window_bytes(), MIN_WINDOW_BYTES);
  }

  #[test]
  fn a_fast_link_is_capped_rather_than_unbounded() {
    let (mut pacer, clock) = pacer_at(0);
    for i in 1..=4u64 {
      clock.advance(Duration::from_millis(100));
      pacer.observe(i * 1024 * 1024);
    }
    assert_eq!(pacer.window_bytes(), MAX_WINDOW_BYTES);
  }

  #[test]
  fn a_mid_rate_link_gets_its_measured_budget() {
    let (mut pacer, clock) = pacer_at(0);
    for i in 1..=4u64 {
      clock.advance(Duration::from_secs(1));
      pacer.observe(i * 200 * 1024);
    }
    let window = pacer.window_bytes();
    assert!(window > MIN_WINDOW_BYTES && window < MAX_WINDOW_BYTES, "got {window}");
    assert_eq!(window, (200.0 * 1024.0 * 0.6_f64).round() as u64);
  }

  #[test]
  fn a_replayed_or_stale_total_never_moves_the_window() {
    let (mut pacer, clock) = pacer_at(0);
    clock.advance(Duration::from_secs(1));
    pacer.observe(200 * 1024);
    let settled = pacer.window_bytes();
    pacer.observe(200 * 1024);
    pacer.observe(1024);
    assert_eq!(pacer.window_bytes(), settled);
  }

  #[test]
  fn a_non_advancing_ack_yields_no_rate_estimate_at_all() {
    let (mut pacer, clock) = pacer_at(8 * 1024);
    clock.advance(Duration::from_secs(5));
    pacer.observe(8 * 1024);
    pacer.observe(4 * 1024);
    assert_eq!(pacer.rate_per_sec(), None, "no rate estimate without progress");
    assert_eq!(pacer.window_bytes(), MIN_WINDOW_BYTES);
  }

  #[test]
  fn the_pacer_reaches_link_rate_over_bluetooth() {
    let link = 175_000.0;
    let (throughput, _) = simulate(link, 0.25, 60.0);
    assert!(
      throughput > link * 0.9,
      "pacer must not be the constraint on a link this slow; got {throughput} B/s of {link}"
    );
  }

  #[test]
  fn the_pacer_reaches_link_rate_when_the_round_trip_is_long() {
    let link = 175_000.0;
    let (throughput, _) = simulate(link, 0.5, 120.0);
    assert!(throughput > link * 0.9, "got {throughput} B/s of {link}");
  }

  #[test]
  fn the_window_stays_inside_the_queueing_budget() {
    let link = 175_000.0;
    let (_, window) = simulate(link, 0.25, 60.0);
    let queued = window as f64 / link;
    assert!(
      queued <= TARGET_DELAY.as_secs_f64() * 1.5,
      "queued {queued}s of link time"
    );
  }

  #[test]
  fn the_window_stays_inside_the_receivers_buffered_depth() {
    let (_, window) = simulate(20_000_000.0, 0.002, 5.0);
    assert!(window <= MAX_WINDOW_BYTES);
    assert!(window / FRAGMENT_BYTES as u64 <= 16);
  }

  #[test]
  fn a_transient_stall_does_not_collapse_the_window() {
    let (mut pacer, clock) = pacer_at(0);
    let mut acked = 0u64;
    for _ in 0..8 {
      clock.advance_secs(0.25);
      acked += 44 * 1024;
      pacer.observe(acked);
    }
    let settled = pacer.window_bytes();
    assert!(settled > MIN_WINDOW_BYTES);

    clock.advance_secs(4.0);
    acked += 4 * 1024;
    pacer.observe(acked);
    assert_eq!(
      pacer.window_bytes(),
      settled,
      "one slow sample must not shed the window"
    );
  }

  #[test]
  fn sustained_degradation_does_shrink_the_window() {
    let (mut pacer, clock) = pacer_at(0);
    let mut acked = 0u64;
    for _ in 0..8 {
      clock.advance_secs(0.25);
      acked += 128 * 1024;
      pacer.observe(acked);
    }
    let fast = pacer.window_bytes();
    for _ in 0..RATE_SAMPLES {
      clock.advance_secs(2.0);
      acked += 8 * 1024;
      pacer.observe(acked);
    }
    assert!(
      pacer.window_bytes() < fast,
      "a link that is genuinely slow now must queue less"
    );
    assert!(pacer.window_bytes() >= MIN_WINDOW_BYTES);
  }

  #[test]
  fn a_resume_baseline_does_not_invent_a_huge_first_sample() {
    let (mut pacer, clock) = pacer_at(30 * 1024 * 1024);
    assert_eq!(pacer.acked(), 30 * 1024 * 1024, "the resume point is the baseline");
    clock.advance_secs(0.25);
    pacer.observe(30 * 1024 * 1024 + 44 * 1024);
    let rate = pacer.rate_per_sec().unwrap_or(0.0);
    assert!(rate < 1_000_000.0, "rate came out as {rate} B/s, so the baseline was 0");
  }

  #[test]
  fn an_ack_below_the_resume_baseline_is_ignored() {
    let (mut pacer, clock) = pacer_at(64 * 1024);
    clock.advance_secs(1.0);
    pacer.observe(16 * 1024);
    assert_eq!(pacer.acked(), 64 * 1024);
    assert_eq!(pacer.rate_per_sec(), None);
    assert_eq!(pacer.window_bytes(), MIN_WINDOW_BYTES);
  }

  #[test]
  fn a_zero_length_interval_is_charged_one_millisecond() {
    let (mut pacer, _clock) = pacer_at(0);
    pacer.observe(1024);
    assert_eq!(
      pacer.rate_per_sec(),
      Some(1024.0 / MIN_SAMPLE_SECONDS),
      "an instantaneous ack must not report an infinite rate"
    );
  }

  #[test]
  fn the_sample_window_evicts_the_oldest() {
    let (mut pacer, clock) = pacer_at(0);
    let mut acked = 0u64;
    clock.advance_secs(0.1);
    acked += 512 * 1024;
    pacer.observe(acked);
    let peak = pacer.rate_per_sec().unwrap();
    for _ in 0..RATE_SAMPLES {
      clock.advance_secs(1.0);
      acked += 16 * 1024;
      pacer.observe(acked);
    }
    assert!(
      pacer.rate_per_sec().unwrap() < peak,
      "the fast sample must age out after {RATE_SAMPLES} newer ones"
    );
  }
}

#[cfg(test)]
mod trace {
  use std::{fs, path::PathBuf};

  use serde::{Deserialize, Serialize};

  use super::{super::fixture::TestClock, *};

  #[derive(Deserialize)]
  struct Corpus {
    cases: Vec<Case>,
  }

  #[derive(Deserialize)]
  struct Case {
    name: String,
    steps: Vec<Step>,
  }

  #[derive(Deserialize)]
  struct Step {
    t_ms: u64,
    observe: Option<u64>,
  }

  #[derive(Serialize)]
  struct Emitted {
    #[serde(rename = "impl")]
    implementation: &'static str,
    constants: Constants,
    cases: Vec<EmittedCase>,
  }

  #[derive(Serialize)]
  struct Constants {
    target_delay_ms: u64,
    ack_interval_bytes: u64,
    min_window_bytes: u64,
    max_window_bytes: u64,
    rate_sample_count: usize,
    fragment_bytes: Option<u64>,
  }

  #[derive(Serialize)]
  struct EmittedCase {
    name: String,
    steps: Vec<EmittedStep>,
  }

  #[derive(Serialize)]
  struct EmittedStep {
    t_ms: u64,
    window_bytes: u64,
    rate_micros: Option<i64>,
  }

  #[derive(Deserialize)]
  struct Expectation {
    constants: serde_json::Map<String, serde_json::Value>,
    asymmetries: serde_json::Map<String, serde_json::Value>,
    cases: Vec<ExpectedCase>,
  }

  #[derive(Deserialize)]
  struct ExpectedCase {
    name: String,
    steps: Vec<ExpectedStep>,
  }

  #[derive(Deserialize)]
  struct ExpectedStep {
    t_ms: u64,
    window_bytes: u64,
    rate_micros: Option<i64>,
  }

  fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../lib/fixtures")
  }

  fn run_corpus() -> Emitted {
    let corpus: Corpus =
      serde_json::from_str(&fs::read_to_string(fixtures_dir().join("pacer-trace.json")).expect("corpus readable"))
        .expect("corpus parses");

    let mut cases = Vec::new();
    for case in corpus.cases {
      let clock = TestClock::new();
      let mut pacer = Pacer::new(clock.clone(), 0);
      let mut at_ms = 0u64;
      let mut steps = Vec::new();

      for step in case.steps {
        if step.t_ms > at_ms {
          clock.advance(Duration::from_millis(step.t_ms - at_ms));
          at_ms = step.t_ms;
        }
        if let Some(acked) = step.observe {
          pacer.observe(acked);
        }
        steps.push(EmittedStep {
          t_ms: step.t_ms,
          window_bytes: pacer.window_bytes(),
          rate_micros: pacer.rate_per_sec().map(|rate| (rate * 1e6).round() as i64),
        });
      }

      cases.push(EmittedCase { name: case.name, steps });
    }

    Emitted {
      implementation: "rust",
      constants: Constants {
        target_delay_ms: TARGET_DELAY.as_millis() as u64,
        ack_interval_bytes: ACK_INTERVAL_BYTES,
        min_window_bytes: MIN_WINDOW_BYTES,
        max_window_bytes: MAX_WINDOW_BYTES,
        rate_sample_count: RATE_SAMPLES,
        fragment_bytes: Some(FRAGMENT_BYTES as u64),
      },
      cases,
    }
  }

  #[test]
  fn emits_pacer_trace() {
    let emitted = run_corpus();
    fs::write(
      fixtures_dir().join("pacer-trace.rust.json"),
      format!("{}\n", serde_json::to_string_pretty(&emitted).expect("serializes")),
    )
    .expect("trace written");
  }

  #[test]
  fn conforms_to_the_frozen_expectation() {
    let expectation: Expectation = serde_json::from_str(
      &fs::read_to_string(fixtures_dir().join("pacer-trace.expected.json")).expect("expectation readable"),
    )
    .expect("expectation parses");
    let emitted = run_corpus();

    let constants = serde_json::to_value(&emitted.constants).expect("constants serialize");
    let constants = constants.as_object().expect("constants are an object");

    let mut declared: Vec<&str> = expectation
      .constants
      .keys()
      .chain(expectation.asymmetries.keys())
      .map(String::as_str)
      .collect();
    declared.sort_unstable();
    let mut present: Vec<&str> = constants.keys().map(String::as_str).collect();
    present.sort_unstable();
    assert_eq!(
      present, declared,
      "pacer constants moved; reconcile them into the expectation"
    );

    for (key, want) in &expectation.constants {
      assert_eq!(constants.get(key), Some(want), "constant {key}");
    }

    assert_eq!(emitted.cases.len(), expectation.cases.len(), "case count");
    for (got, want) in emitted.cases.iter().zip(&expectation.cases) {
      assert_eq!(got.name, want.name, "case order");
      assert_eq!(got.steps.len(), want.steps.len(), "step count in {}", want.name);
      for (g, w) in got.steps.iter().zip(&want.steps) {
        assert_eq!(
          (g.t_ms, g.window_bytes, g.rate_micros),
          (w.t_ms, w.window_bytes, w.rate_micros),
          "{} at t={}ms",
          want.name,
          w.t_ms
        );
      }
    }
  }
}
