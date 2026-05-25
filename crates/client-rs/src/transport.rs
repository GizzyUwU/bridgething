//! WebSocket + JSON transport for the client (webapp) protocol. The
//! daemon's local client server speaks JSON over text frames; each frame
//! is one complete `{ id, meta, data }` envelope, so no cross-frame
//! buffering is needed and `recv` is trivially cancel-safe.

use bridgething_sdk_runtime::{Connector, InboundHalf, OutboundHalf, TransportError};
use futures::{
  SinkExt, StreamExt,
  stream::{SplitSink, SplitStream},
};
use libbridgething::{
  client::{BridgeToClientMsg, ClientToBridgeMsg},
  protocol::PrioritizedFrame,
};
use tokio::net::TcpStream;
use tokio_tungstenite::{
  MaybeTlsStream, WebSocketStream,
  tungstenite::{Error as WsError, Message},
};

fn map_ws(err: WsError) -> TransportError {
  match err {
    WsError::ConnectionClosed | WsError::AlreadyClosed => TransportError::Closed,
    WsError::Io(io) => TransportError::Io(io),
    other => TransportError::Decode(other.to_string()),
  }
}

pub type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub struct WsConnector {
  pub(crate) ws: Ws,
}

pub struct WsOut {
  sink: SplitSink<Ws, Message>,
}

pub struct WsIn {
  stream: SplitStream<Ws>,
}

impl Connector<crate::ClientProtocol> for WsConnector {
  type Out = WsOut;
  type In = WsIn;
  fn split(self) -> (WsOut, WsIn) {
    let (sink, stream) = self.ws.split();
    (WsOut { sink }, WsIn { stream })
  }
}

impl OutboundHalf<crate::ClientProtocol> for WsOut {
  // the client lane has no priority wire field; priority is dropped.
  async fn send(&mut self, frame: PrioritizedFrame<ClientToBridgeMsg>) -> Result<(), TransportError> {
    let text = serde_json::to_string(&frame.msg).map_err(|e| TransportError::Encode(e.to_string()))?;
    self.sink.send(Message::Text(text.into())).await.map_err(map_ws)
  }
}

impl InboundHalf<crate::ClientProtocol> for WsIn {
  async fn recv(&mut self) -> Option<Result<BridgeToClientMsg, TransportError>> {
    loop {
      match self.stream.next().await {
        Some(Ok(Message::Text(text))) => {
          return Some(serde_json::from_str(text.as_str()).map_err(|e| TransportError::Decode(e.to_string())));
        }
        Some(Ok(Message::Binary(bytes))) => {
          return Some(serde_json::from_slice(&bytes).map_err(|e| TransportError::Decode(e.to_string())));
        }
        Some(Ok(Message::Close(_))) | None => return None,
        Some(Ok(_)) => continue,
        Some(Err(err)) => return Some(Err(map_ws(err))),
      }
    }
  }
}
