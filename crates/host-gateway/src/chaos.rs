//! Failure-injection and link-conditioning knobs the connection + OTA
//! modules consult before shipping an outbound frame. Probabilistic loss
//! runs through a single shared `rand` thread RNG; the disconnect timer is
//! owned by the connection driver and races the WS reader/writer; the
//! throttle paces the outbound writer to emulate a slow BT link.

use std::time::Duration;

use rand::{Rng, rng};

#[derive(Debug, Clone, Copy)]
pub struct ChaosConfig {
  pub inject_loss: f32,
  pub inject_disconnect: Option<Duration>,
  pub throttle_bytes_per_sec: Option<u64>,
}

impl ChaosConfig {
  pub fn should_drop(&self) -> bool {
    if self.inject_loss <= 0.0 {
      return false;
    }
    let p = self.inject_loss.clamp(0.0, 1.0);
    rng().random::<f32>() < p
  }
}
