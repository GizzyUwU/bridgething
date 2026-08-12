pub const BACKOFF_BASE_MS: u64 = 120_000;
pub const BACKOFF_MAX_MS: u64 = 15 * 60 * 1_000;
pub const BACKOFF_SHIFT_CAP: u32 = 5;
pub const BACKOFF_JITTER_MS: u64 = 0;
pub const LINK_STABILITY_MS: u64 = 120_000;
pub const MIN_RESUME_DELAY_MS: u64 = 5_000;
pub const MIN_POLL_INTERVAL_SECONDS: u64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
  pub raw_ms: u64,
  pub delay_ms: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AutoPushSchedule {
  failures: u32,
  next_at_ms: Option<u64>,
  link_opened_at_ms: Option<u64>,
}

impl AutoPushSchedule {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn failures(&self) -> u32 {
    self.failures
  }

  pub fn next_at_ms(&self) -> Option<u64> {
    self.next_at_ms
  }

  pub fn record_failure(&mut self, now_ms: u64) -> Backoff {
    self.failures += 1;
    let raw_ms = BACKOFF_BASE_MS << (self.failures - 1).min(BACKOFF_SHIFT_CAP);
    let delay_ms = raw_ms.min(BACKOFF_MAX_MS);
    self.next_at_ms = Some(now_ms + delay_ms);
    Backoff { raw_ms, delay_ms }
  }

  pub fn record_success(&mut self) {
    self.failures = 0;
    self.next_at_ms = None;
  }

  pub fn link_opened(&mut self, now_ms: u64) {
    self.link_opened_at_ms = Some(now_ms);
  }

  pub fn link_closed(&mut self) {
    self.link_opened_at_ms = None;
  }

  pub fn link_stable(&self, now_ms: u64) -> bool {
    self
      .link_opened_at_ms
      .is_some_and(|opened| now_ms.saturating_sub(opened) >= LINK_STABILITY_MS)
  }

  pub fn ready(&self, now_ms: u64) -> bool {
    self.link_stable(now_ms) && now_ms >= self.next_at_ms.unwrap_or(0)
  }

  pub fn wake_deadline_ms(&self, now_ms: u64, interval_seconds: u64) -> u64 {
    let mut deadline = now_ms + interval_seconds.max(MIN_POLL_INTERVAL_SECONDS) * 1_000;

    if let Some(next_at) = self.next_at_ms.filter(|next_at| *next_at < deadline) {
      deadline = next_at.max(now_ms + MIN_RESUME_DELAY_MS);
    }

    if let Some(opened) = self.link_opened_at_ms {
      let stable_at = opened + LINK_STABILITY_MS;
      if stable_at > now_ms && stable_at < deadline {
        deadline = stable_at;
      }
    }

    deadline
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const NOW: u64 = 1_700_000_000_000;

  #[test]
  fn a_fresh_schedule_owes_nothing() {
    let schedule = AutoPushSchedule::new();

    assert_eq!(schedule.failures(), 0);
    assert_eq!(schedule.next_at_ms(), None);
    assert!(!schedule.link_stable(NOW), "a schedule with no link has no stable link");
    assert!(!schedule.ready(NOW), "and is therefore not ready");
  }

  #[test]
  fn the_backoff_ladder_doubles_then_holds_at_the_ceiling() {
    let mut schedule = AutoPushSchedule::new();
    let mut raw = Vec::new();
    let mut delay = Vec::new();

    for _ in 0..8 {
      let backoff = schedule.record_failure(NOW);
      raw.push(backoff.raw_ms);
      delay.push(backoff.delay_ms);
    }

    assert_eq!(
      raw,
      vec![
        120_000, 240_000, 480_000, 960_000, 1_920_000, 3_840_000, 3_840_000, 3_840_000
      ]
    );
    assert_eq!(
      delay,
      vec![120_000, 240_000, 480_000, 900_000, 900_000, 900_000, 900_000, 900_000]
    );
  }

  #[test]
  fn a_failure_arms_the_next_attempt_at_the_delay() {
    let mut schedule = AutoPushSchedule::new();

    let backoff = schedule.record_failure(NOW);

    assert_eq!(schedule.failures(), 1);
    assert_eq!(schedule.next_at_ms(), Some(NOW + backoff.delay_ms));
  }

  #[test]
  fn a_success_clears_the_ladder() {
    let mut schedule = AutoPushSchedule::new();
    schedule.record_failure(NOW);
    schedule.record_failure(NOW);

    schedule.record_success();

    assert_eq!(schedule.failures(), 0);
    assert_eq!(schedule.next_at_ms(), None);
  }

  #[test]
  fn a_link_is_not_worth_a_multi_megabyte_update_until_it_has_held() {
    let mut schedule = AutoPushSchedule::new();
    schedule.link_opened(NOW);

    assert!(!schedule.link_stable(NOW + LINK_STABILITY_MS - 1));
    assert!(schedule.link_stable(NOW + LINK_STABILITY_MS));
  }

  #[test]
  fn a_closed_link_is_never_stable_again() {
    let mut schedule = AutoPushSchedule::new();
    schedule.link_opened(NOW);
    schedule.link_closed();

    assert!(!schedule.link_stable(NOW + LINK_STABILITY_MS * 10));
  }

  #[test]
  fn readiness_needs_both_a_held_link_and_an_expired_backoff() {
    let mut schedule = AutoPushSchedule::new();
    schedule.link_opened(NOW);
    schedule.record_failure(NOW);
    let backoff = schedule.record_failure(NOW);
    assert!(backoff.delay_ms > LINK_STABILITY_MS);

    assert!(!schedule.ready(NOW + LINK_STABILITY_MS), "the backoff has not expired");
    assert!(!schedule.ready(NOW + backoff.delay_ms - 1));
    assert!(schedule.ready(NOW + backoff.delay_ms));
  }

  #[test]
  fn a_backoff_that_has_expired_still_needs_a_link_that_has_held() {
    let mut schedule = AutoPushSchedule::new();
    let backoff = schedule.record_failure(NOW);
    schedule.link_opened(NOW + backoff.delay_ms);

    assert!(!schedule.ready(NOW + backoff.delay_ms), "the link only just came up");
    assert!(schedule.ready(NOW + backoff.delay_ms + LINK_STABILITY_MS));
  }

  #[test]
  fn the_wake_deadline_falls_back_to_the_configured_cadence() {
    let schedule = AutoPushSchedule::new();

    assert_eq!(schedule.wake_deadline_ms(NOW, 3_600), NOW + 3_600_000);
  }

  #[test]
  fn the_cadence_is_floored_at_a_minute() {
    let schedule = AutoPushSchedule::new();

    assert_eq!(
      schedule.wake_deadline_ms(NOW, 5),
      NOW + MIN_POLL_INTERVAL_SECONDS * 1_000
    );
  }

  #[test]
  fn an_armed_backoff_pulls_the_wake_in() {
    let mut schedule = AutoPushSchedule::new();
    schedule.record_failure(NOW);

    assert_eq!(schedule.wake_deadline_ms(NOW, 3_600), NOW + BACKOFF_BASE_MS);
  }

  #[test]
  fn an_already_due_backoff_still_waits_out_the_resume_floor() {
    let mut schedule = AutoPushSchedule::new();
    schedule.record_failure(NOW);
    let due = NOW + BACKOFF_BASE_MS;

    assert_eq!(
      schedule.wake_deadline_ms(due, 3_600),
      due + MIN_RESUME_DELAY_MS,
      "a due backoff must not spin the poll loop"
    );
  }

  #[test]
  fn a_link_about_to_count_as_stable_pulls_the_wake_in() {
    let mut schedule = AutoPushSchedule::new();
    schedule.link_opened(NOW);

    assert_eq!(
      schedule.wake_deadline_ms(NOW + 1_000, 3_600),
      NOW + LINK_STABILITY_MS,
      "the loop wakes exactly when the link becomes pushable"
    );
  }

  #[test]
  fn an_already_stable_link_does_not_pull_the_wake_in() {
    let mut schedule = AutoPushSchedule::new();
    schedule.link_opened(NOW);
    let long_stable = NOW + LINK_STABILITY_MS * 2;

    assert_eq!(schedule.wake_deadline_ms(long_stable, 3_600), long_stable + 3_600_000);
  }

  #[test]
  fn there_is_no_jitter_in_the_ladder() {
    assert_eq!(BACKOFF_JITTER_MS, 0);
    assert_eq!(BACKOFF_SHIFT_CAP, 5);
    assert_eq!(BACKOFF_MAX_MS, 900_000);
  }
}
