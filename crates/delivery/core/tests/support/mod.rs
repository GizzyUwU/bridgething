use std::sync::{
  Arc,
  atomic::{AtomicU64, Ordering},
};

use bridgething_delivery::seam::Clock;
use bridgething_sdk_runtime::rt::Instant;

pub struct TestClock {
  millis: AtomicU64,
}

impl TestClock {
  pub fn at(millis: u64) -> Arc<Self> {
    Arc::new(Self {
      millis: AtomicU64::new(millis),
    })
  }

  pub fn set(&self, millis: u64) {
    self.millis.store(millis, Ordering::SeqCst);
  }
}

impl Clock for TestClock {
  fn now(&self) -> Instant {
    Instant::now()
  }

  fn unix_millis(&self) -> u64 {
    self.millis.load(Ordering::SeqCst)
  }
}
