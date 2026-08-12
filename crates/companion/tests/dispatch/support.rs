use std::{
  sync::{Arc, Mutex},
  time::Duration,
};

use bridgething_gateway::Gateway;
use futures::StreamExt;
use libbridgething::{
  gateway::GatewayToBridgeMsg,
  protocol::{BridgeEndec, DecodedFrame},
};
use tokio_util::codec::Framed;

pub const ARRIVAL: Duration = Duration::from_secs(5);
pub const POLL: Duration = Duration::from_millis(5);

pub struct Peer {
  pub seen: Arc<Mutex<Vec<GatewayToBridgeMsg>>>,
}

impl Peer {
  pub fn link() -> (Gateway, Peer) {
    let (near, far) = tokio::io::duplex(256 * 1024);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    tokio::spawn(async move {
      let mut framed = Framed::new(far, BridgeEndec::default());
      while let Some(Ok(DecodedFrame::Frame(frame))) = framed.next().await {
        sink.lock().unwrap().push(frame.msg);
      }
    });
    (Gateway::from_io(near), Peer { seen })
  }

  pub fn scan<T>(&self, pick: &impl Fn(&GatewayToBridgeMsg) -> Option<T>) -> Option<T> {
    self.seen.lock().unwrap().iter().find_map(pick)
  }

  pub async fn wait<T>(&self, what: &str, pick: impl Fn(&GatewayToBridgeMsg) -> Option<T>) -> T {
    let deadline = tokio::time::Instant::now() + ARRIVAL;
    loop {
      if let Some(found) = self.scan(&pick) {
        return found;
      }
      if tokio::time::Instant::now() >= deadline {
        panic!("no {what} arrived; the peer heard {:?}", self.seen.lock().unwrap());
      }
      tokio::time::sleep(POLL).await;
    }
  }
}
