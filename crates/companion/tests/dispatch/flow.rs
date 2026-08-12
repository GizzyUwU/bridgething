use std::time::Duration;

use libbridgething::gateway::GatewayToBridgeMsg;

use crate::support::{ARRIVAL, POLL, Peer};

impl Peer {
  pub async fn wait_for<T>(
    &self,
    what: &str,
    wanted: usize,
    pick: impl Fn(&GatewayToBridgeMsg) -> Option<T>,
  ) -> Duration {
    let started = tokio::time::Instant::now();
    loop {
      let arrived = self.seen.lock().unwrap().iter().filter_map(&pick).count();
      if arrived >= wanted {
        return started.elapsed();
      }
      assert!(started.elapsed() < ARRIVAL, "only {arrived} of {wanted} {what} arrived");
      tokio::time::sleep(POLL).await;
    }
  }
}
