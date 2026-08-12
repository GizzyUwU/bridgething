mod codec;
pub mod routing;
#[cfg(not(target_arch = "wasm32"))]
pub mod transport;
#[cfg(target_arch = "wasm32")]
pub mod wasm;

#[path = "surface.generated.rs"]
mod surface;
use std::time::Duration;

use bridgething_sdk_runtime::{Connection, Connector, LaneLimits, Protocol};
pub use bridgething_sdk_runtime::{MsgHandle, Reply, RequestFailure, SdkError, TransportError};
use libbridgething::{
  Priority,
  gateway::{BridgeToGatewayMsg, BridgeToGatewayMsgData, GatewayToBridgeMsg, GatewayToBridgeMsgData},
  wire::{MsgMeta, WireCommand, WireError, WireEvent, WireRequest},
};
pub use surface::*;
#[cfg(not(target_arch = "wasm32"))]
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::broadcast;
#[cfg(all(not(target_arch = "wasm32"), feature = "native-ws"))]
use tokio_tungstenite::connect_async;
#[cfg(all(not(target_arch = "wasm32"), feature = "native-ws"))]
pub use transport::Ws;
#[cfg(not(target_arch = "wasm32"))]
pub use transport::connect_seam_ws;
use uuid::Uuid;

#[cfg(all(not(target_arch = "wasm32"), feature = "native-ws"))]
pub async fn connect_ws(url: &str) -> Result<transport::Ws, TransportError> {
  let dial = tokio::time::timeout(transport::WS_CONNECT_TIMEOUT, connect_async(url))
    .await
    .map_err(|_| transport::connect_timed_out())?;
  let (ws, _) = dial.map_err(|e| TransportError::Decode(format!("ws connect: {e}")))?;
  Ok(ws)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandlerError<E> {
  Domain(E),
  Wire(WireError),
}

impl<E> From<WireError> for HandlerError<E> {
  fn from(error: WireError) -> Self {
    Self::Wire(error)
  }
}

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

#[derive(Clone)]
pub struct Gateway {
  conn: Connection<GatewayProtocol>,
}

impl Gateway {
  pub fn spawn<C: Connector<GatewayProtocol>>(connector: C) -> Self {
    Self::spawn_subscribed(connector).0
  }

  pub fn spawn_subscribed<C: Connector<GatewayProtocol>>(
    connector: C,
  ) -> (Self, broadcast::Receiver<BridgeToGatewayMsg>) {
    let (conn, events) = Connection::spawn_subscribed(connector, LaneLimits::default());
    (Self { conn }, events)
  }

  #[cfg(not(target_arch = "wasm32"))]
  pub fn from_io<S>(io: S) -> Self
  where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
  {
    Self::spawn(transport::FramedConnector::new(io))
  }

  #[cfg(any(target_arch = "wasm32", feature = "native-ws"))]
  pub async fn connect(url: &str) -> Result<Self, TransportError> {
    Ok(Self::connect_subscribed(url).await?.0)
  }

  #[cfg(all(not(target_arch = "wasm32"), feature = "native-ws"))]
  pub async fn connect_subscribed(
    url: &str,
  ) -> Result<(Self, broadcast::Receiver<BridgeToGatewayMsg>), TransportError> {
    Ok(Self::spawn_subscribed(transport::WsConnector::new(
      connect_ws(url).await?,
    )))
  }

  #[cfg(target_arch = "wasm32")]
  pub async fn connect_subscribed(
    url: &str,
  ) -> Result<(Self, broadcast::Receiver<BridgeToGatewayMsg>), TransportError> {
    Ok(Self::spawn_subscribed(wasm::connect_websocket(url).await?))
  }

  pub fn with_timeout(self, timeout: Duration) -> Self {
    Self {
      conn: self.conn.with_timeout(timeout),
    }
  }

  pub fn events(&self) -> broadcast::Receiver<BridgeToGatewayMsg> {
    self.conn.events()
  }

  pub fn connection(&self) -> &Connection<GatewayProtocol> {
    &self.conn
  }

  pub fn handle(&self, msg: &BridgeToGatewayMsg) -> MsgHandle<GatewayProtocol> {
    self.conn.handle(msg)
  }

  pub async fn event<E>(&self, event: E) -> Result<(), SdkError>
  where
    E: WireEvent<GatewayToBridgeMsgData>,
  {
    self.conn.event(event).await
  }

  pub async fn command<C>(&self, command: C) -> Result<(), SdkError>
  where
    C: WireCommand<GatewayToBridgeMsgData>,
  {
    self.conn.command(command).await
  }

  pub async fn request<R>(&self, request: R) -> Result<R::Response, RequestFailure<R::DomainError>>
  where
    R: WireRequest<Outbound = GatewayToBridgeMsgData, Inbound = BridgeToGatewayMsgData>,
  {
    self.conn.request(request).await
  }

  pub async fn send_data(
    &self,
    meta: MsgMeta,
    data: GatewayToBridgeMsgData,
    priority: Priority,
  ) -> Result<(), SdkError> {
    self.conn.send_data(meta, data, priority).await
  }
}

#[async_trait::async_trait]
pub trait OutboundLink: Send + Sync {
  async fn send_data(&self, meta: MsgMeta, data: GatewayToBridgeMsgData, priority: Priority) -> Result<(), SdkError>;
}

#[async_trait::async_trait]
impl OutboundLink for Gateway {
  async fn send_data(&self, meta: MsgMeta, data: GatewayToBridgeMsgData, priority: Priority) -> Result<(), SdkError> {
    self.conn.send_data(meta, data, priority).await
  }
}

#[async_trait::async_trait]
pub trait OutboundLinkExt: OutboundLink {
  async fn event<E>(&self, event: E) -> Result<(), SdkError>
  where
    E: WireEvent<GatewayToBridgeMsgData> + Send,
  {
    self.send_data(MsgMeta::Event, event.into(), Priority::Normal).await
  }

  async fn command<C>(&self, command: C) -> Result<(), SdkError>
  where
    C: WireCommand<GatewayToBridgeMsgData> + Send,
  {
    self.send_data(MsgMeta::Command, command.into(), Priority::Normal).await
  }
}

#[async_trait::async_trait]
impl<T: OutboundLink + ?Sized> OutboundLinkExt for T {}

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use futures::{SinkExt, StreamExt};
  use libbridgething::{
    GatewayCapabilities, GatewayInfo,
    gateway::{
      BridgeToGatewayMsg, BridgeToGatewayMsgData, GatewayToBridgeCapabilitiesMsgEvent, GatewayToBridgeMsgData,
    },
    protocol::{BridgeEndec, DecodedFrame},
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
        if let Ok(DecodedFrame::Frame(frame)) = item {
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
      let _ = framed.next().await;
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
      while let Some(Ok(DecodedFrame::Frame(frame))) = framed.next().await {
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
