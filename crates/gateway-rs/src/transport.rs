use std::{sync::Arc, time::Duration};

use bridgething_io as io;
use bridgething_sdk_runtime::{Connector, InboundHalf, OutboundHalf, TransportError};
use futures::StreamExt;
#[cfg(feature = "native-ws")]
use futures::{
  SinkExt,
  stream::{SplitSink, SplitStream},
};
use libbridgething::{
  gateway::{BridgeToGatewayMsg, GatewayToBridgeMsg},
  protocol::{DecodedFrame, GatewayEndec, PrioritizedFrame},
};
#[cfg(feature = "native-ws")]
use tokio::net::TcpStream;
use tokio::{
  io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadHalf, WriteHalf},
  sync::mpsc,
};
#[cfg(feature = "native-ws")]
use tokio_tungstenite::{
  MaybeTlsStream, WebSocketStream,
  tungstenite::{Error as WsError, Message},
};
use tokio_util::{
  bytes::{Bytes, BytesMut},
  codec::FramedRead,
};
use uuid::Uuid;

use crate::{
  GatewayProtocol,
  codec::{BATCH_BYTES, decode_step, encode_frame, map_endec},
};

#[cfg(feature = "native-ws")]
fn map_ws(err: WsError) -> TransportError {
  match err {
    WsError::ConnectionClosed | WsError::AlreadyClosed => TransportError::Closed,
    WsError::Io(io) => TransportError::Io(io),
    other => TransportError::Decode(other.to_string()),
  }
}

pub struct FramedConnector<S> {
  io: S,
}

impl<S> FramedConnector<S> {
  pub fn new(io: S) -> Self {
    FramedConnector { io }
  }
}

pub struct FramedOut<S> {
  writer: WriteHalf<S>,
}

pub struct FramedIn<S> {
  stream: FramedRead<ReadHalf<S>, GatewayEndec>,
}

impl<S> Connector<GatewayProtocol> for FramedConnector<S>
where
  S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
  type Out = FramedOut<S>;
  type In = FramedIn<S>;
  fn split(self) -> (FramedOut<S>, FramedIn<S>) {
    let (read, writer) = tokio::io::split(self.io);
    (
      FramedOut { writer },
      FramedIn {
        stream: FramedRead::new(read, GatewayEndec::default()),
      },
    )
  }
}

impl<S> OutboundHalf<GatewayProtocol> for FramedOut<S>
where
  S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
  fn max_batch_bytes(&self) -> usize {
    BATCH_BYTES
  }

  fn encode(frame: PrioritizedFrame<GatewayToBridgeMsg>) -> Result<Bytes, TransportError> {
    encode_frame(frame)
  }

  async fn ready(&mut self) -> Result<(), TransportError> {
    Ok(())
  }

  async fn send_batch(&mut self, batch: Bytes) -> Result<(), TransportError> {
    self.writer.write_all(&batch).await.map_err(TransportError::Io)?;
    self.writer.flush().await.map_err(TransportError::Io)
  }
}

impl<S> InboundHalf<GatewayProtocol> for FramedIn<S>
where
  S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
  async fn recv(&mut self) -> Option<Result<BridgeToGatewayMsg, TransportError>> {
    match self.stream.next().await {
      Some(Ok(DecodedFrame::Frame(frame))) => Some(Ok(frame.msg)),
      Some(Ok(DecodedFrame::Failed(err))) | Some(Err(err)) => Some(Err(map_endec(err))),
      None => None,
    }
  }
}

#[cfg(feature = "native-ws")]
pub type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[cfg(feature = "native-ws")]
pub struct WsConnector {
  ws: Ws,
}

#[cfg(feature = "native-ws")]
impl WsConnector {
  pub fn new(ws: Ws) -> Self {
    WsConnector { ws }
  }
}

#[cfg(feature = "native-ws")]
pub struct WsOut {
  sink: SplitSink<Ws, Message>,
}

#[cfg(feature = "native-ws")]
pub struct WsIn {
  stream: SplitStream<Ws>,
  decoder: GatewayEndec,
  buf: BytesMut,
}

#[cfg(feature = "native-ws")]
impl Connector<GatewayProtocol> for WsConnector {
  type Out = WsOut;
  type In = WsIn;
  fn split(self) -> (WsOut, WsIn) {
    let (sink, stream) = self.ws.split();
    (
      WsOut { sink },
      WsIn {
        stream,
        decoder: GatewayEndec::default(),
        buf: BytesMut::new(),
      },
    )
  }
}

#[cfg(feature = "native-ws")]
impl OutboundHalf<GatewayProtocol> for WsOut {
  fn max_batch_bytes(&self) -> usize {
    BATCH_BYTES
  }

  fn encode(frame: PrioritizedFrame<GatewayToBridgeMsg>) -> Result<Bytes, TransportError> {
    encode_frame(frame)
  }

  async fn ready(&mut self) -> Result<(), TransportError> {
    futures::future::poll_fn(|cx| self.sink.poll_ready_unpin(cx))
      .await
      .map_err(map_ws)
  }

  async fn send_batch(&mut self, batch: Bytes) -> Result<(), TransportError> {
    self.sink.send(Message::Binary(batch)).await.map_err(map_ws)
  }
}

#[cfg(feature = "native-ws")]
impl InboundHalf<GatewayProtocol> for WsIn {
  async fn recv(&mut self) -> Option<Result<BridgeToGatewayMsg, TransportError>> {
    loop {
      if let Some(result) = decode_step(&mut self.decoder, &mut self.buf) {
        return Some(result);
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

pub const WS_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) fn connect_timed_out() -> TransportError {
  TransportError::Io(std::io::Error::new(
    std::io::ErrorKind::TimedOut,
    format!("ws connect timed out after {}s", WS_CONNECT_TIMEOUT.as_secs()),
  ))
}

struct Socket {
  transport: Arc<dyn io::WsTransport>,
  id: Uuid,
}

impl Drop for Socket {
  fn drop(&mut self) {
    self.transport.disconnect(self.id, None, None);
  }
}

pub struct SeamWsConnector {
  socket: Arc<Socket>,
  rx: mpsc::UnboundedReceiver<io::WsEvent>,
  buf: BytesMut,
}

pub async fn connect_seam_ws(
  transport: Arc<dyn io::WsTransport>,
  url: &str,
) -> Result<SeamWsConnector, TransportError> {
  let (tx, mut rx) = mpsc::unbounded_channel();
  let id = Uuid::now_v7();
  transport.connect(
    io::WsConnect {
      id,
      url: url.to_string(),
      protocols: Vec::new(),
      headers: Vec::new(),
    },
    Arc::new(io::WsInbox::new(tx)),
  );
  let socket = Arc::new(Socket { transport, id });

  let handshake = async {
    let mut buf = BytesMut::new();
    loop {
      match rx.recv().await {
        Some(io::WsEvent::Open { .. }) => return Ok(buf),
        Some(io::WsEvent::Frame {
          frame: io::WsFrame::Binary(bytes),
          ..
        }) => {
          buf.extend_from_slice(&bytes);
          return Ok(buf);
        }
        Some(io::WsEvent::Frame { .. }) => continue,
        Some(io::WsEvent::Closed { reason, .. }) => {
          return Err(TransportError::Decode(format!("ws connect: {reason}")));
        }
        None => return Err(TransportError::Closed),
      }
    }
  };
  let buf = match tokio::time::timeout(WS_CONNECT_TIMEOUT, handshake).await {
    Ok(opened) => opened?,
    Err(_) => return Err(connect_timed_out()),
  };

  Ok(SeamWsConnector { socket, rx, buf })
}

pub struct SeamWsOut {
  socket: Arc<Socket>,
}

pub struct SeamWsIn {
  _socket: Arc<Socket>,
  rx: mpsc::UnboundedReceiver<io::WsEvent>,
  decoder: GatewayEndec,
  buf: BytesMut,
}

impl Connector<GatewayProtocol> for SeamWsConnector {
  type Out = SeamWsOut;
  type In = SeamWsIn;
  fn split(self) -> (SeamWsOut, SeamWsIn) {
    (
      SeamWsOut {
        socket: self.socket.clone(),
      },
      SeamWsIn {
        _socket: self.socket,
        rx: self.rx,
        decoder: GatewayEndec::default(),
        buf: self.buf,
      },
    )
  }
}

impl OutboundHalf<GatewayProtocol> for SeamWsOut {
  fn max_batch_bytes(&self) -> usize {
    BATCH_BYTES
  }

  fn encode(frame: PrioritizedFrame<GatewayToBridgeMsg>) -> Result<Bytes, TransportError> {
    encode_frame(frame)
  }

  async fn ready(&mut self) -> Result<(), TransportError> {
    Ok(())
  }

  async fn send_batch(&mut self, batch: Bytes) -> Result<(), TransportError> {
    self
      .socket
      .transport
      .send(self.socket.id, io::WsFrame::Binary(batch.to_vec()));
    Ok(())
  }
}

impl InboundHalf<GatewayProtocol> for SeamWsIn {
  async fn recv(&mut self) -> Option<Result<BridgeToGatewayMsg, TransportError>> {
    loop {
      if let Some(result) = decode_step(&mut self.decoder, &mut self.buf) {
        return Some(result);
      }
      match self.rx.recv().await {
        Some(io::WsEvent::Frame {
          frame: io::WsFrame::Binary(bytes),
          ..
        }) => self.buf.extend_from_slice(&bytes),
        Some(io::WsEvent::Frame { .. }) | Some(io::WsEvent::Open { .. }) => continue,
        Some(io::WsEvent::Closed { .. }) | None => return None,
      }
    }
  }
}
