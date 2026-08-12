use std::time::Duration;

use libbridgething::gateway::GatewayToBridgeMsg;

use crate::support::Peer;

const QUIET: Duration = Duration::from_millis(400);

impl Peer {
  pub async fn settled_count<T>(&self, pick: impl Fn(&GatewayToBridgeMsg) -> Option<T>) -> usize {
    tokio::time::sleep(QUIET).await;
    self.seen.lock().unwrap().iter().filter_map(&pick).count()
  }
}
