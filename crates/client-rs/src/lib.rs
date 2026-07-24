mod transport;

#[path = "surface.generated.rs"]
mod surface;
use std::time::Duration;

use bridgething_sdk_runtime::{Connection, Protocol};
pub use bridgething_sdk_runtime::{MsgHandle, RequestFailure, SdkError, TransportError};
use libbridgething::{
  Priority,
  client::{BridgeToClientMsg, BridgeToClientMsgData, ClientToBridgeMsg, ClientToBridgeMsgData},
  wire::{MsgMeta, WireCommand, WireEvent, WireRequest},
};
pub use surface::*;
use tokio::sync::broadcast;
use tokio_tungstenite::connect_async;
pub use transport::Ws;
use uuid::Uuid;

pub const DEFAULT_URL: &str = "ws://127.0.0.1:8891/";

pub struct ClientProtocol;

impl Protocol for ClientProtocol {
  type OutData = ClientToBridgeMsgData;
  type InData = BridgeToClientMsgData;
  type OutMsg = ClientToBridgeMsg;
  type InMsg = BridgeToClientMsg;

  fn envelope(id: Uuid, meta: MsgMeta, data: ClientToBridgeMsgData) -> ClientToBridgeMsg {
    ClientToBridgeMsg { id, meta, data }
  }
  fn in_id(msg: &BridgeToClientMsg) -> Uuid {
    msg.id
  }
  fn in_meta(msg: &BridgeToClientMsg) -> &MsgMeta {
    &msg.meta
  }
  fn in_data(msg: BridgeToClientMsg) -> BridgeToClientMsgData {
    msg.data
  }
}

#[derive(Clone)]
pub struct Client {
  conn: Connection<ClientProtocol>,
}

impl Client {
  pub async fn connect(url: &str) -> Result<Self, TransportError> {
    let (ws, _) = connect_async(url)
      .await
      .map_err(|e| TransportError::Decode(format!("ws connect: {e}")))?;
    Ok(Self {
      conn: Connection::spawn(transport::WsConnector { ws }),
    })
  }

  pub fn with_timeout(self, timeout: Duration) -> Self {
    Self {
      conn: self.conn.with_timeout(timeout),
    }
  }

  pub fn events(&self) -> broadcast::Receiver<BridgeToClientMsg> {
    self.conn.events()
  }

  pub fn connection(&self) -> &Connection<ClientProtocol> {
    &self.conn
  }

  pub fn handle(&self, msg: &BridgeToClientMsg) -> MsgHandle<ClientProtocol> {
    self.conn.handle(msg)
  }

  pub async fn event<E>(&self, event: E) -> Result<(), SdkError>
  where
    E: WireEvent<ClientToBridgeMsgData>,
  {
    self.conn.event(event).await
  }

  pub async fn command<C>(&self, command: C) -> Result<(), SdkError>
  where
    C: WireCommand<ClientToBridgeMsgData>,
  {
    self.conn.command(command).await
  }

  pub async fn request<R>(&self, request: R) -> Result<R::Response, RequestFailure<R::DomainError>>
  where
    R: WireRequest<Outbound = ClientToBridgeMsgData, Inbound = BridgeToClientMsgData>,
  {
    self.conn.request(request).await
  }

  pub async fn send_data(
    &self,
    meta: MsgMeta,
    data: ClientToBridgeMsgData,
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
    PlayerState,
    client::{
      BridgeToClientMsg, BridgeToClientMsgData, BridgeToClientPlayerMsg, BridgeToClientPlayerMsgEvent,
      ClientToBridgeMsg, ClientToBridgeMsgData, ClientToBridgePlayerMsg, PlayerStateGet, PlayerStateReply,
    },
    wire::{MsgMeta, ResponseMeta},
  };
  use tokio::net::TcpListener;
  use tokio_tungstenite::tungstenite::Message;

  use super::*;

  async fn fake_daemon(seen: tokio::sync::mpsc::UnboundedSender<ClientToBridgeMsg>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
      let (tcp, _) = listener.accept().await.unwrap();
      let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
      while let Some(Ok(msg)) = ws.next().await {
        let Message::Text(text) = msg else { continue };
        let incoming: ClientToBridgeMsg = serde_json::from_str(text.as_str()).unwrap();
        if let (MsgMeta::Request, ClientToBridgeMsgData::Player(ClientToBridgePlayerMsg::StateGet)) =
          (&incoming.meta, &incoming.data)
        {
          let reply = BridgeToClientMsg {
            id: uuid::Uuid::now_v7(),
            meta: MsgMeta::Response(ResponseMeta {
              request_id: incoming.id,
            }),
            data: BridgeToClientMsgData::Player(BridgeToClientPlayerMsg::StateReply(PlayerStateReply {
              state: PlayerState::default(),
              active_app: None,
            })),
          };
          ws.send(Message::Text(serde_json::to_string(&reply).unwrap().into()))
            .await
            .unwrap();
        }
        let _ = seen.send(incoming);
      }
    });
    format!("ws://{addr}/")
  }

  #[tokio::test]
  async fn named_surface_command_is_sent_as_json() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let url = fake_daemon(tx).await;
    let client = Client::connect(&url).await.expect("connect");

    client.player().pause().await.expect("send pause");

    let seen = tokio::time::timeout(Duration::from_secs(1), rx.recv())
      .await
      .expect("not timed out")
      .expect("a message");
    assert!(matches!(
      seen.data,
      ClientToBridgeMsgData::Player(ClientToBridgePlayerMsg::Pause)
    ));
    assert!(matches!(seen.meta, MsgMeta::Command));
  }

  #[tokio::test]
  async fn named_surface_events_stream_yields_typed_events() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
      let (tcp, _) = listener.accept().await.unwrap();
      let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
      let _ = ws.next().await;
      let event = BridgeToClientMsg {
        id: uuid::Uuid::now_v7(),
        meta: MsgMeta::Event,
        data: BridgeToClientMsgData::Player(BridgeToClientPlayerMsg::Snapshot(PlayerStateReply {
          state: PlayerState::default(),
          active_app: None,
        })),
      };
      ws.send(Message::Text(serde_json::to_string(&event).unwrap().into()))
        .await
        .unwrap();
      futures::future::pending::<()>().await;
    });

    let client = Client::connect(&format!("ws://{addr}/")).await.expect("connect");
    let mut events = client.player().events();
    client.player().pause().await.expect("trigger far end");

    let event = tokio::time::timeout(Duration::from_secs(1), events.next())
      .await
      .expect("not timed out")
      .expect("an event");
    assert!(matches!(event, BridgeToClientPlayerMsgEvent::Snapshot(_)));
  }

  #[tokio::test]
  async fn typed_request_resolves_over_the_json_lane() {
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let url = fake_daemon(tx).await;
    let client = Client::connect(&url).await.expect("connect");

    let reply = client.request(PlayerStateGet).await.expect("state reply");
    assert_eq!(
      reply,
      PlayerStateReply {
        state: PlayerState::default(),
        active_app: None,
      }
    );
  }
}
