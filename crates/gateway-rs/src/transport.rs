//! Concrete transports for the gateway (companion) protocol.
//!
//! - [`FramedConnector`] frames `GatewayEndec` over any
//!   `AsyncRead + AsyncWrite` (a duplex pair in-process, a real RFCOMM
//!   socket on hardware).
//! - [`WsConnector`] carries the same `GatewayEndec` frames as binary
//!   WebSocket messages (the daemon's network gateway on port 8892).

use bridgething_sdk_runtime::{Connector, InboundHalf, OutboundHalf, TransportError};
use futures::{
  SinkExt, StreamExt,
  stream::{SplitSink, SplitStream},
};
use libbridgething::{
  gateway::{BridgeToGatewayMsg, GatewayToBridgeMsg},
  protocol::{EndecError, GatewayEndec, PrioritizedFrame},
};
use tokio::{
  io::{AsyncRead, AsyncWrite},
  net::TcpStream,
};
use tokio_tungstenite::{
  MaybeTlsStream, WebSocketStream,
  tungstenite::{Error as WsError, Message},
};
use tokio_util::{
  bytes::BytesMut,
  codec::{Decoder, Encoder, Framed},
};

use crate::GatewayProtocol;

fn map_endec(err: EndecError) -> TransportError {
  match err {
    EndecError::Io(io) => TransportError::Io(io),
    other => TransportError::Decode(other.to_string()),
  }
}

fn map_ws(err: WsError) -> TransportError {
  match err {
    WsError::ConnectionClosed | WsError::AlreadyClosed => TransportError::Closed,
    WsError::Io(io) => TransportError::Io(io),
    other => TransportError::Decode(other.to_string()),
  }
}

pub struct FramedConnector<S> {
  pub(crate) io: S,
}

pub struct FramedOut<S> {
  sink: SplitSink<Framed<S, GatewayEndec>, PrioritizedFrame<GatewayToBridgeMsg>>,
}

pub struct FramedIn<S> {
  stream: SplitStream<Framed<S, GatewayEndec>>,
}

impl<S> Connector<GatewayProtocol> for FramedConnector<S>
where
  S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
  type Out = FramedOut<S>;
  type In = FramedIn<S>;
  fn split(self) -> (FramedOut<S>, FramedIn<S>) {
    let (sink, stream) = Framed::new(self.io, GatewayEndec::default()).split::<PrioritizedFrame<GatewayToBridgeMsg>>();
    (FramedOut { sink }, FramedIn { stream })
  }
}

impl<S> OutboundHalf<GatewayProtocol> for FramedOut<S>
where
  S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
  async fn send(&mut self, frame: PrioritizedFrame<GatewayToBridgeMsg>) -> Result<(), TransportError> {
    self.sink.send(frame).await.map_err(map_endec)
  }
}

impl<S> InboundHalf<GatewayProtocol> for FramedIn<S>
where
  S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
  async fn recv(&mut self) -> Option<Result<BridgeToGatewayMsg, TransportError>> {
    match self.stream.next().await {
      Some(Ok(frame)) => Some(Ok(frame.msg)),
      Some(Err(err)) => Some(Err(map_endec(err))),
      None => None,
    }
  }
}

pub type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub struct WsConnector {
  pub(crate) ws: Ws,
}

pub struct WsOut {
  sink: SplitSink<Ws, Message>,
  encoder: GatewayEndec,
}

pub struct WsIn {
  stream: SplitStream<Ws>,
  decoder: GatewayEndec,
  buf: BytesMut,
}

impl Connector<GatewayProtocol> for WsConnector {
  type Out = WsOut;
  type In = WsIn;
  fn split(self) -> (WsOut, WsIn) {
    let (sink, stream) = self.ws.split();
    (
      WsOut {
        sink,
        encoder: GatewayEndec::default(),
      },
      WsIn {
        stream,
        decoder: GatewayEndec::default(),
        buf: BytesMut::new(),
      },
    )
  }
}

impl OutboundHalf<GatewayProtocol> for WsOut {
  async fn send(&mut self, frame: PrioritizedFrame<GatewayToBridgeMsg>) -> Result<(), TransportError> {
    let mut dst = BytesMut::new();
    self.encoder.encode(frame, &mut dst).map_err(map_endec)?;
    self.sink.send(Message::Binary(dst.freeze())).await.map_err(map_ws)
  }
}

impl InboundHalf<GatewayProtocol> for WsIn {
  async fn recv(&mut self) -> Option<Result<BridgeToGatewayMsg, TransportError>> {
    loop {
      match self.decoder.decode(&mut self.buf) {
        Ok(Some(frame)) => return Some(Ok(frame.msg)),
        Ok(None) => {}
        Err(err) => return Some(Err(map_endec(err))),
      }
      match self.stream.next().await {
        Some(Ok(Message::Binary(bytes))) => self.buf.extend_from_slice(&bytes),
        Some(Ok(Message::Close(_))) | None => return None,
        Some(Ok(_)) => continue,
        Some(Err(err)) => return Some(Err(map_ws(err))),
      }
    }
  }
}
