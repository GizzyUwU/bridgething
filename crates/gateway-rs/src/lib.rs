//! Ergonomic Rust SDK for the gateway (companion) side of the
//! bridgething wire protocol. A companion speaks `GatewayToBridgeMsg`
//! outbound and receives `BridgeToGatewayMsg`. Built on
//! [`bridgething_sdk_runtime`]: the generic `event` / `command` /
//! `request` methods are type-checked against the wire marker traits,
//! and the named per-surface methods are layered on top by codegen.

mod transport;

#[path = "surface.generated.rs"]
mod surface;
use std::time::Duration;

use bridgething_sdk_runtime::{Connection, Protocol};
pub use bridgething_sdk_runtime::{MsgHandle, RequestFailure, SdkError, TransportError};
use libbridgething::{
  Priority,
  gateway::{BridgeToGatewayMsg, BridgeToGatewayMsgData, GatewayToBridgeMsg, GatewayToBridgeMsgData},
  wire::{MsgMeta, WireCommand, WireEvent, WireRequest},
};
pub use surface::*;
use tokio::{
  io::{AsyncRead, AsyncWrite},
  sync::broadcast,
};
use tokio_tungstenite::connect_async;
pub use transport::Ws;
use uuid::Uuid;

/// Wire-protocol binding for the companion side.
pub struct GatewayProtocol;

impl Protocol for GatewayProtocol {
  type OutData = GatewayToBridgeMsgData;
  type InData = BridgeToGatewayMsgData;
  type OutMsg = GatewayToBridgeMsg;
  type InMsg = BridgeToGatewayMsg;

  fn envelope(id: Uuid, meta: MsgMeta, data: GatewayToBridgeMsgData) -> GatewayToBridgeMsg {
    GatewayToBridgeMsg { id, meta, data }
  }
  fn in_id(msg: &BridgeToGatewayMsg) -> Uuid {
    msg.id
  }
  fn in_meta(msg: &BridgeToGatewayMsg) -> &MsgMeta {
    &msg.meta
  }
  fn in_data(msg: BridgeToGatewayMsg) -> BridgeToGatewayMsgData {
    msg.data
  }
}

/// A connected gateway.
#[derive(Clone)]
pub struct Gateway {
  conn: Connection<GatewayProtocol>,
}

impl Gateway {
  /// Drive the protocol over any byte stream.
  pub fn from_io<S>(io: S) -> Self
  where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
  {
    Self {
      conn: Connection::spawn(transport::FramedConnector { io }),
    }
  }

  /// Connect to a daemon's network gateway over WebSocket.
  pub async fn connect(url: &str) -> Result<Self, TransportError> {
    let (ws, _) = connect_async(url)
      .await
      .map_err(|e| TransportError::Decode(format!("ws connect: {e}")))?;
    Ok(Self {
      conn: Connection::spawn(transport::WsConnector { ws }),
    })
  }

  /// Override the per-request response timeout (default 30s).
  pub fn with_timeout(self, timeout: Duration) -> Self {
    Self {
      conn: self.conn.with_timeout(timeout),
    }
  }

  /// Inbound messages that aren't correlated responses (events, daemon-initiated requests, broadcasts).
  pub fn events(&self) -> broadcast::Receiver<BridgeToGatewayMsg> {
    self.conn.events()
  }

  /// Raw connection handle for callers that want the generic surface directly.
  pub fn connection(&self) -> &Connection<GatewayProtocol> {
    &self.conn
  }

  /// Build a per-message handle for an inbound message
  pub fn handle(&self, msg: &BridgeToGatewayMsg) -> MsgHandle<GatewayProtocol> {
    self.conn.handle(msg)
  }

  /// Fire-and-forget event (`meta = event`).
  pub async fn event<E>(&self, event: E) -> Result<(), SdkError>
  where
    E: WireEvent<GatewayToBridgeMsgData>,
  {
    self.conn.event(event).await
  }

  /// Fire-and-forget command (`meta = command`).
  pub async fn command<C>(&self, command: C) -> Result<(), SdkError>
  where
    C: WireCommand<GatewayToBridgeMsgData>,
  {
    self.conn.command(command).await
  }

  /// Typed request: await the correlated, decoded response.
  pub async fn request<R>(&self, request: R) -> Result<R::Response, RequestFailure<R::DomainError>>
  where
    R: WireRequest<Outbound = GatewayToBridgeMsgData, Inbound = BridgeToGatewayMsgData>,
  {
    self.conn.request(request).await
  }

  /// Escape hatch: send arbitrary out-data with a chosen meta + priority.
  pub async fn send_data(
    &self,
    meta: MsgMeta,
    data: GatewayToBridgeMsgData,
    priority: Priority,
  ) -> Result<(), SdkError> {
    self.conn.send_data(meta, data, priority).await
  }
}

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use futures::{SinkExt, StreamExt};
  use libbridgething::{
    GatewayCapabilities, GatewayInfo,
    gateway::{
      BridgeToGatewayMsg, BridgeToGatewayMsgData, GatewayToBridgeCapabilitiesMsgEvent, GatewayToBridgeMsgData,
    },
    protocol::BridgeEndec,
    wire::{MsgMeta, ResponseMeta, WireError},
  };
  use tokio_util::codec::Framed;
  use uuid::Uuid;

  use super::*;

  fn announce() -> GatewayToBridgeCapabilitiesMsgEvent {
    GatewayToBridgeCapabilitiesMsgEvent::Announce(GatewayCapabilities {
      gateway: GatewayInfo {
        address: String::new(),
        name: "sdk-test".into(),
        os_name: "linux".into(),
        app_name: "sdk-test".into(),
        app_version: "0.0.0".into(),
        adapter_version: "test".into(),
        lib_version: "0.0.0".into(),
        libbridgething_version: format!("v{}", libbridgething::LIBBRIDGETHING_VERSION),
      },
      ..Default::default()
    })
  }

  #[tokio::test]
  async fn typed_event_encodes_and_decodes_on_the_far_end() {
    let (client_io, daemon_io) = tokio::io::duplex(256 * 1024);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
      let mut framed = Framed::new(daemon_io, BridgeEndec::default());
      while let Some(item) = framed.next().await {
        if let Ok(frame) = item {
          let _ = tx.send(frame.msg);
        }
      }
    });

    let gw = Gateway::from_io(client_io);
    gw.event(announce()).await.expect("send announce");

    let got = tokio::time::timeout(Duration::from_secs(1), rx.recv())
      .await
      .expect("not timed out")
      .expect("a frame");
    assert!(matches!(
      got.data,
      GatewayToBridgeMsgData::Capabilities(libbridgething::gateway::GatewayToBridgeCapabilitiesMsg::Announce(_))
    ));
  }

  #[tokio::test]
  async fn inbound_event_reaches_events_stream() {
    let (client_io, daemon_io) = tokio::io::duplex(256 * 1024);
    tokio::spawn(async move {
      let mut framed = Framed::new(daemon_io, BridgeEndec::default());
      // wait for the gateway's first outbound frame, then push an event back.
      let _ = framed.next().await;
      framed
        .send(BridgeToGatewayMsg {
          id: Uuid::now_v7(),
          meta: MsgMeta::Event,
          data: BridgeToGatewayMsgData::Error(WireError::Unsupported),
        })
        .await
        .expect("send back");
      futures::future::pending::<()>().await;
    });

    let gw = Gateway::from_io(client_io);
    let mut events = gw.events();
    gw.event(announce()).await.expect("send announce");

    let got = tokio::time::timeout(Duration::from_secs(1), events.recv())
      .await
      .expect("not timed out")
      .expect("an event");
    assert!(matches!(
      got.data,
      BridgeToGatewayMsgData::Error(WireError::Unsupported)
    ));
  }

  #[tokio::test]
  async fn inbound_request_answered_via_handle() {
    use libbridgething::{
      HttpMethod, NetFetchRequest, NetFetchResponse, RedirectPolicy,
      gateway::{BridgeToGatewayNetMsg, GatewayToBridgeNetMsg, NetFetchReply, NetFetchRequestMsg},
    };

    let (client_io, daemon_io) = tokio::io::duplex(256 * 1024);
    let req_id = Uuid::now_v7();
    let (resp_tx, mut resp_rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
      let mut framed = Framed::new(daemon_io, BridgeEndec::default());
      let _ = framed.next().await; // gateway's announce - subscription is live by now
      let request = BridgeToGatewayMsg {
        id: req_id,
        meta: MsgMeta::Request,
        data: BridgeToGatewayMsgData::Net(BridgeToGatewayNetMsg::Fetch(NetFetchRequestMsg {
          request: NetFetchRequest {
            url: "https://example.com".into(),
            method: HttpMethod::Get,
            headers: vec![],
            body: None,
            timeout_ms: None,
            redirect: RedirectPolicy::Follow,
          },
        })),
      };
      framed.send(request).await.expect("send request");
      while let Some(Ok(frame)) = framed.next().await {
        let _ = resp_tx.send(frame.msg);
      }
    });

    let gw = Gateway::from_io(client_io);
    let mut inbound = gw.events();
    gw.event(announce()).await.expect("announce");

    let msg = tokio::time::timeout(Duration::from_secs(1), inbound.recv())
      .await
      .expect("not timed out")
      .expect("a request");
    let handle = gw.handle(&msg);
    assert!(handle.is_request());
    handle
      .respond_to::<NetFetchRequestMsg>(NetFetchReply {
        response: NetFetchResponse {
          status: 200,
          headers: vec![],
          body: b"ok".to_vec(),
        },
      })
      .await
      .expect("respond");

    let resp = tokio::time::timeout(Duration::from_secs(1), resp_rx.recv())
      .await
      .expect("not timed out")
      .expect("a response");
    assert!(matches!(resp.meta, MsgMeta::Response(ResponseMeta { request_id }) if request_id == req_id));
    assert!(matches!(
      resp.data,
      GatewayToBridgeMsgData::Net(GatewayToBridgeNetMsg::FetchReply(_))
    ));
  }
}
