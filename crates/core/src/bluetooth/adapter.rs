use bluer::{Adapter, AdapterEvent, Session};
use futures::{Stream, StreamExt};
use tokio::task::JoinHandle;

use super::{BluetoothError, BluetoothResult, profiles::ProfileMan};

pub struct AdapterEventStream(pub Box<dyn Stream<Item = AdapterEvent> + Send + Unpin>);
impl AdapterEventStream {
  pub fn spawn(self, profile: ProfileMan) -> JoinHandle<()> {
    tracing::debug!("spawning adapter event stream");
    tokio::spawn(async move { self.event_loop(profile).await })
  }

  async fn event_loop(mut self, profile: ProfileMan) {
    loop {
      tokio::select! {
        Some(msg) = self.0.next() => {
          if let Err(err) = profile.handle_event(msg.into()).await {
            tracing::error!("failed to handle adapter event: {:?}", err);
          }
        },
      }
    }
  }
}

pub async fn get_adapter(session: &Session) -> BluetoothResult<Adapter> {
  let timeout = std::time::Duration::new(10, 0);
  let start = std::time::Instant::now();

  let adapter = loop {
    match session.default_adapter().await {
      Ok(adapter) => break adapter,
      Err(e) => {
        if start.elapsed() >= timeout {
          tracing::error!("Error getting default adapter - timing out: {:?}", e);
          return Err(BluetoothError::Timeout);
        }
        tracing::warn!("Error getting default adapter: {:?}", e);
        tokio::time::sleep(std::time::Duration::from_millis(750)).await;
        continue;
      }
    }
  };

  Ok(adapter)
}
