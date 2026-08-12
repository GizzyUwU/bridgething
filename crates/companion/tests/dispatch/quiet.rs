use std::time::Duration;

use libbridgething::gateway::GatewayToBridgeMsg;

use crate::support::{POLL, Peer};

const QUIET: Duration = Duration::from_millis(400);

impl Peer {
  pub async fn quiet<T: std::fmt::Debug>(&self, what: &str, pick: impl Fn(&GatewayToBridgeMsg) -> Option<T>) {
    let deadline = tokio::time::Instant::now() + QUIET;
    while tokio::time::Instant::now() < deadline {
      if let Some(found) = self.scan(&pick) {
        panic!("{what} was not supposed to reach the peer, got {found:?}");
      }
      tokio::time::sleep(POLL).await;
    }
  }
}
