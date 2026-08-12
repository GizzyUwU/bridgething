use std::time::Duration;

use bridgething_gateway::GatewayProtocol;
use bridgething_sdk_runtime::{Connector, OutboundHalf, TransportError, rt};
use libbridgething::{gateway::GatewayToBridgeMsg, protocol::PrioritizedFrame};
use rand::{Rng, rng};
use tokio_util::bytes::Bytes;

#[derive(Debug, Clone, Copy, Default)]
pub struct ChaosConfig {
  pub inject_loss: f32,
  pub inject_disconnect: Option<Duration>,
  pub throttle_bytes_per_sec: Option<u64>,
}

impl ChaosConfig {
  fn should_drop(&self) -> bool {
    if self.inject_loss <= 0.0 {
      return false;
    }
    rng().random::<f32>() < self.inject_loss.clamp(0.0, 1.0)
  }
}

pub struct ChaosConnector<C> {
  inner: C,
  chaos: ChaosConfig,
}

impl<C> ChaosConnector<C> {
  pub fn new(inner: C, chaos: ChaosConfig) -> Self {
    ChaosConnector { inner, chaos }
  }
}

impl<C: Connector<GatewayProtocol>> Connector<GatewayProtocol> for ChaosConnector<C> {
  type Out = ChaosOut<C::Out>;
  type In = C::In;

  fn split(self) -> (Self::Out, Self::In) {
    let (out, inbound) = self.inner.split();
    let hang_up_at = self.chaos.inject_disconnect.map(|after| rt::now() + after);
    (
      ChaosOut {
        out,
        chaos: self.chaos,
        hang_up_at,
      },
      inbound,
    )
  }
}

pub struct ChaosOut<O> {
  out: O,
  chaos: ChaosConfig,
  hang_up_at: Option<rt::Instant>,
}

impl<O> ChaosOut<O> {
  fn hung_up(&self) -> bool {
    self.hang_up_at.is_some_and(|at| rt::now() >= at)
  }
}

impl<O: OutboundHalf<GatewayProtocol>> OutboundHalf<GatewayProtocol> for ChaosOut<O> {
  fn max_batch_bytes(&self) -> usize {
    self.out.max_batch_bytes()
  }

  fn encode(frame: PrioritizedFrame<GatewayToBridgeMsg>) -> Result<Bytes, TransportError> {
    O::encode(frame)
  }

  async fn ready(&mut self) -> Result<(), TransportError> {
    if self.hung_up() {
      return Err(TransportError::Closed);
    }
    self.out.ready().await
  }

  async fn send_batch(&mut self, batch: Bytes) -> Result<(), TransportError> {
    if self.hung_up() {
      tracing::warn!("inject-disconnect deadline passed; dropping the link");
      return Err(TransportError::Closed);
    }
    if let Some(rate) = self.chaos.throttle_bytes_per_sec {
      rt::sleep(Duration::from_secs_f64(batch.len() as f64 / rate.max(1) as f64)).await;
    }
    if self.chaos.should_drop() {
      tracing::warn!(bytes = batch.len(), "inject-loss: dropping an outbound batch");
      return Ok(());
    }
    self.out.send_batch(batch).await
  }
}
