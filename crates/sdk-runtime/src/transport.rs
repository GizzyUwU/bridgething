use std::future::Future;

use bytes::Bytes;
use libbridgething::protocol::PrioritizedFrame;

use crate::{error::TransportError, protocol::Protocol};

pub trait Connector<P: Protocol>: Send + 'static {
  type Out: OutboundHalf<P>;
  type In: InboundHalf<P>;
  fn split(self) -> (Self::Out, Self::In);
}

pub trait OutboundHalf<P: Protocol>: Send + 'static {
  fn max_batch_bytes(&self) -> usize;
  fn encode(frame: PrioritizedFrame<P::OutMsg>) -> Result<Bytes, TransportError>;
  fn ready(&mut self) -> impl Future<Output = Result<(), TransportError>> + Send;
  fn send_batch(&mut self, batch: Bytes) -> impl Future<Output = Result<(), TransportError>> + Send;
}

pub trait InboundHalf<P: Protocol>: Send + 'static {
  fn recv(&mut self) -> impl Future<Output = Option<Result<P::InMsg, TransportError>>> + Send;
}
