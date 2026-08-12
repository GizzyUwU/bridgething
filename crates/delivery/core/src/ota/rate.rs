use std::{collections::VecDeque, sync::Arc};

use crate::seam::Clock;

pub const RATE_WINDOW_MS: u64 = 4_000;
pub const MIN_SPAN_MS: u64 = 50;

pub struct RateTracker {
  clock: Arc<dyn Clock>,
  window_ms: u64,
  samples: VecDeque<(u64, u64)>,
}

impl RateTracker {
  pub fn new(clock: Arc<dyn Clock>) -> Self {
    Self {
      clock,
      window_ms: RATE_WINDOW_MS,
      samples: VecDeque::new(),
    }
  }

  pub fn record(&mut self, bytes: u64) {
    let now = self.clock.unix_millis();
    self.samples.push_back((bytes, now));
    let cutoff = now.saturating_sub(self.window_ms);
    while self.samples.len() > 2 && self.samples.front().is_some_and(|(_, at)| *at < cutoff) {
      self.samples.pop_front();
    }
  }

  pub fn rate_per_sec(&self) -> Option<f64> {
    let (first_bytes, first_at) = *self.samples.front()?;
    let (last_bytes, last_at) = *self.samples.back()?;
    let span_ms = last_at.saturating_sub(first_at);
    if span_ms <= MIN_SPAN_MS || last_bytes < first_bytes {
      return None;
    }
    Some((last_bytes - first_bytes) as f64 / (span_ms as f64 / 1_000.0))
  }

  pub fn eta_seconds(&self, remaining: u64) -> Option<f64> {
    let rate = self.rate_per_sec()?;
    (rate > 0.0).then(|| remaining as f64 / rate)
  }
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use super::RateTracker;
  use crate::{ota::harness::TestClock, seam::Clock};

  fn tracker(clock: Arc<TestClock>) -> RateTracker {
    RateTracker::new(clock)
  }

  async fn advance_ms(millis: u64) {
    tokio::time::advance(std::time::Duration::from_millis(millis)).await;
  }

  #[tokio::test(start_paused = true)]
  async fn one_sample_has_no_rate() {
    let clock = TestClock::new();
    let mut rate = tracker(clock);

    rate.record(1_000);

    assert_eq!(rate.rate_per_sec(), None, "a single sample spans no time");
  }

  #[tokio::test(start_paused = true)]
  async fn two_samples_inside_the_floor_have_no_rate() {
    let clock = TestClock::new();
    let mut rate = tracker(clock);

    rate.record(0);
    advance_ms(50).await;
    rate.record(1_000_000);

    assert_eq!(
      rate.rate_per_sec(),
      None,
      "50ms is the floor both platforms refuse to divide by"
    );
  }

  #[tokio::test(start_paused = true)]
  async fn a_rate_is_bytes_over_the_span_of_the_retained_samples() {
    let clock = TestClock::new();
    let mut rate = tracker(clock);

    rate.record(0);
    advance_ms(1_000).await;
    rate.record(500_000);

    assert_eq!(rate.rate_per_sec(), Some(500_000.0));
  }

  #[tokio::test(start_paused = true)]
  async fn a_shrinking_byte_count_has_no_rate() {
    let clock = TestClock::new();
    let mut rate = tracker(clock);

    rate.record(900_000);
    advance_ms(1_000).await;
    rate.record(100);

    assert_eq!(
      rate.rate_per_sec(),
      None,
      "a restarted transfer must not report a negative rate"
    );
  }

  #[tokio::test(start_paused = true)]
  async fn samples_older_than_the_window_stop_counting() {
    let clock = TestClock::new();
    let mut rate = tracker(clock);

    let mut sent = 0u64;
    rate.record(sent);
    advance_ms(100).await;
    sent += 1_000_000;
    rate.record(sent);

    for _ in 0..5 {
      advance_ms(1_000).await;
      sent += 100_000;
      rate.record(sent);
    }

    let measured = rate.rate_per_sec().expect("a rate after five slow seconds");
    assert!(
      measured < 200_000.0,
      "the opening burst aged out of the window, got {measured}"
    );
  }

  #[tokio::test(start_paused = true)]
  async fn two_samples_always_survive_eviction() {
    let clock = TestClock::new();
    let mut rate = tracker(clock);

    rate.record(0);
    advance_ms(1_000).await;
    rate.record(100_000);
    advance_ms(60_000).await;

    assert_eq!(
      rate.rate_per_sec(),
      Some(100_000.0),
      "a stalled transfer keeps its last measured rate rather than losing all samples"
    );
  }

  #[tokio::test(start_paused = true)]
  async fn an_eta_is_the_remainder_over_the_rate() {
    let clock = TestClock::new();
    let mut rate = tracker(clock);

    rate.record(0);
    advance_ms(1_000).await;
    rate.record(250_000);

    assert_eq!(rate.eta_seconds(500_000), Some(2.0));
  }

  #[tokio::test(start_paused = true)]
  async fn no_rate_means_no_eta() {
    let clock = TestClock::new();
    let rate = tracker(clock);

    assert_eq!(rate.eta_seconds(500_000), None);
  }

  #[tokio::test(start_paused = true)]
  async fn a_zero_rate_means_no_eta() {
    let clock = TestClock::new();
    let mut rate = tracker(clock);

    rate.record(1_000);
    advance_ms(1_000).await;
    rate.record(1_000);

    assert_eq!(
      rate.eta_seconds(500_000),
      None,
      "a stopped transfer has no finish time, not an infinite one"
    );
  }

  #[tokio::test(start_paused = true)]
  async fn the_tracker_reads_the_injected_clock_and_not_the_host() {
    let clock = TestClock::new();
    let before = clock.unix_millis();
    let mut rate = tracker(clock.clone());

    rate.record(0);
    advance_ms(2_000).await;
    rate.record(2_000);

    assert_eq!(clock.unix_millis() - before, 2_000);
    assert_eq!(rate.rate_per_sec(), Some(1_000.0));
  }
}
