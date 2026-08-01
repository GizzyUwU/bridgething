use std::{collections::VecDeque, time::Duration};

use tokio::time::Instant;

const TARGET_DELAY: Duration = Duration::from_millis(600);
const ACK_INTERVAL_BYTES: u64 = 16 * 1024;
const MIN_WINDOW_BYTES: u64 = 4 * ACK_INTERVAL_BYTES;
const MAX_WINDOW_BYTES: u64 = 16 * ACK_INTERVAL_BYTES;
const RATE_SAMPLES: usize = 8;

#[derive(Debug)]
pub struct Pacer {
  acked: u64,
  last_progress: Instant,
  samples: VecDeque<f64>,
}

impl Pacer {
  pub fn new() -> Self {
    Self {
      acked: 0,
      last_progress: Instant::now(),
      samples: VecDeque::with_capacity(RATE_SAMPLES),
    }
  }

  pub fn observe(&mut self, acked: u64) {
    if acked <= self.acked {
      return;
    }
    let now = Instant::now();
    let elapsed = (now - self.last_progress).as_secs_f64().max(0.001);
    if self.samples.len() == RATE_SAMPLES {
      self.samples.pop_front();
    }
    self.samples.push_back((acked - self.acked) as f64 / elapsed);
    self.acked = acked;
    self.last_progress = now;
  }

  pub fn window_bytes(&self) -> u64 {
    let Some(rate) = self
      .samples
      .iter()
      .copied()
      .fold(None::<f64>, |acc, s| Some(acc.map_or(s, |a: f64| a.max(s))))
    else {
      return MIN_WINDOW_BYTES;
    };
    let budget = (rate * TARGET_DELAY.as_secs_f64()).round() as u64;
    budget.clamp(MIN_WINDOW_BYTES, MAX_WINDOW_BYTES)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test(start_paused = true)]
  async fn an_unmeasured_pacer_opens_at_the_floor() {
    let pacer = Pacer::new();
    assert_eq!(pacer.window_bytes(), MIN_WINDOW_BYTES);
  }

  #[tokio::test(start_paused = true)]
  async fn a_slow_link_stays_at_the_floor() {
    let mut pacer = Pacer::new();
    for i in 1..=4u64 {
      tokio::time::advance(Duration::from_secs(1)).await;
      pacer.observe(i * 16 * 1024);
    }
    assert_eq!(pacer.window_bytes(), MIN_WINDOW_BYTES);
  }

  #[tokio::test(start_paused = true)]
  async fn a_fast_link_is_capped_rather_than_unbounded() {
    let mut pacer = Pacer::new();
    for i in 1..=4u64 {
      tokio::time::advance(Duration::from_millis(100)).await;
      pacer.observe(i * 1024 * 1024);
    }
    assert_eq!(pacer.window_bytes(), MAX_WINDOW_BYTES);
  }

  #[tokio::test(start_paused = true)]
  async fn a_mid_rate_link_gets_its_measured_budget() {
    let mut pacer = Pacer::new();
    for i in 1..=4u64 {
      tokio::time::advance(Duration::from_secs(1)).await;
      pacer.observe(i * 200 * 1024);
    }
    let window = pacer.window_bytes();
    assert!(window > MIN_WINDOW_BYTES && window < MAX_WINDOW_BYTES, "got {window}");
    assert_eq!(window, (200.0 * 1024.0 * 0.6_f64).round() as u64);
  }

  #[tokio::test(start_paused = true)]
  async fn a_replayed_or_stale_total_never_moves_the_window() {
    let mut pacer = Pacer::new();
    tokio::time::advance(Duration::from_secs(1)).await;
    pacer.observe(200 * 1024);
    let settled = pacer.window_bytes();
    pacer.observe(200 * 1024);
    pacer.observe(1024);
    assert_eq!(pacer.window_bytes(), settled);
  }
}

#[cfg(test)]
mod trace {
  use std::{fs, path::PathBuf};

  use serde::{Deserialize, Serialize};

  use super::*;

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

  fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../lib/fixtures")
  }

  fn rate_micros(pacer: &Pacer) -> Option<i64> {
    pacer
      .samples
      .iter()
      .copied()
      .fold(None::<f64>, |acc, s| Some(acc.map_or(s, |a: f64| a.max(s))))
      .map(|rate| (rate * 1e6).round() as i64)
  }

  #[tokio::test(start_paused = true)]
  async fn emits_pacer_trace() {
    let dir = fixtures_dir();
    let corpus: Corpus =
      serde_json::from_str(&fs::read_to_string(dir.join("pacer-trace.json")).expect("corpus readable"))
        .expect("corpus parses");

    let mut cases = Vec::new();
    for case in corpus.cases {
      let mut pacer = Pacer::new();
      let mut at_ms = 0u64;
      let mut steps = Vec::new();

      for step in case.steps {
        if step.t_ms > at_ms {
          tokio::time::advance(Duration::from_millis(step.t_ms - at_ms)).await;
          at_ms = step.t_ms;
        }
        if let Some(acked) = step.observe {
          pacer.observe(acked);
        }
        steps.push(EmittedStep {
          t_ms: step.t_ms,
          window_bytes: pacer.window_bytes(),
          rate_micros: rate_micros(&pacer),
        });
      }

      cases.push(EmittedCase { name: case.name, steps });
    }

    let emitted = Emitted {
      implementation: "rust",
      constants: Constants {
        target_delay_ms: TARGET_DELAY.as_millis() as u64,
        ack_interval_bytes: ACK_INTERVAL_BYTES,
        min_window_bytes: MIN_WINDOW_BYTES,
        max_window_bytes: MAX_WINDOW_BYTES,
        rate_sample_count: RATE_SAMPLES,
        fragment_bytes: None,
      },
      cases,
    };

    fs::write(
      dir.join("pacer-trace.rust.json"),
      format!("{}\n", serde_json::to_string_pretty(&emitted).expect("serializes")),
    )
    .expect("trace written");
  }
}
