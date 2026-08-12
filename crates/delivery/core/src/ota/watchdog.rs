use std::sync::{
  Arc,
  atomic::{AtomicU64, Ordering},
};

use crate::seam::Clock;

pub const IDLE_DEADLINE_MS: u64 = 60_000;

pub const IDLE_POLL_MS: u64 = 15_000;

pub fn stalled_reason() -> String {
  format!("ota stalled: no progress within {}s", IDLE_DEADLINE_MS / 1_000)
}

pub struct ProgressClock {
  clock: Arc<dyn Clock>,
  last_ms: AtomicU64,
}

impl ProgressClock {
  pub fn new(clock: Arc<dyn Clock>) -> Self {
    let last_ms = AtomicU64::new(clock.unix_millis());
    Self { clock, last_ms }
  }

  pub fn touch(&self) {
    self.last_ms.store(self.clock.unix_millis(), Ordering::SeqCst);
  }

  pub fn idle_ms(&self) -> u64 {
    self
      .clock
      .unix_millis()
      .saturating_sub(self.last_ms.load(Ordering::SeqCst))
  }
}

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use super::{IDLE_DEADLINE_MS, IDLE_POLL_MS, ProgressClock, stalled_reason};
  use crate::ota::harness::TestClock;

  async fn advance_ms(millis: u64) {
    tokio::time::advance(Duration::from_millis(millis)).await;
  }

  #[test]
  fn the_deadline_and_its_poll_are_the_shipped_numbers() {
    assert_eq!(IDLE_DEADLINE_MS, 60_000);
    assert_eq!(IDLE_POLL_MS, 15_000);
    assert_eq!(stalled_reason(), "ota stalled: no progress within 60s");
  }

  #[tokio::test(start_paused = true)]
  async fn a_fresh_clock_is_not_idle() {
    let progress = ProgressClock::new(TestClock::new());

    assert_eq!(progress.idle_ms(), 0);
  }

  #[tokio::test(start_paused = true)]
  async fn idle_time_is_measured_from_the_last_touch() {
    let progress = ProgressClock::new(TestClock::new());

    advance_ms(30_000).await;
    progress.touch();
    advance_ms(20_000).await;

    assert_eq!(
      progress.idle_ms(),
      20_000,
      "a touch restarts the count, it does not add to it"
    );
  }

  #[tokio::test(start_paused = true)]
  async fn the_deadline_is_exclusive() {
    let progress = ProgressClock::new(TestClock::new());

    advance_ms(IDLE_DEADLINE_MS).await;

    assert_eq!(
      progress.idle_ms(),
      IDLE_DEADLINE_MS,
      "exactly the deadline is not yet past it, which is what both platforms compare"
    );
  }
}
