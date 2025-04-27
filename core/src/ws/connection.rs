use futures::{SinkExt, StreamExt};
use std::net::SocketAddr;
use tokio::{net::TcpStream, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tokio_websockets::WebSocketStream;
use uuid::Uuid;

use crate::msg::{ClientMode, PossibleRecvMsg, PossibleSendMsg, RecvMsg, RecvMsgData, RecvTx, SendRx};

pub struct Connection {
  mode: ClientMode,
  address: SocketAddr,
  stream: WebSocketStream<TcpStream>,
  cancel_token: CancellationToken,

  tx: RecvTx,
  rx: SendRx,
}

impl Connection {
  pub fn spawn(
    address: SocketAddr,
    stream: WebSocketStream<TcpStream>,
    tx: RecvTx,
    rx: SendRx,
    cancel_token: CancellationToken,
  ) -> JoinHandle<()> {
    tracing::debug!("spawning listener for {address} in default modern mode");
    tokio::spawn(async move {
      Self {
        mode: ClientMode::Modern,
        address,
        stream,
        cancel_token,

        tx,
        rx,
      }
      .listen()
      .await
    })
  }

  pub async fn listen(&mut self) {
    loop {
      tokio::select! {
        ws_msg = self.stream.next() => {
          let Some(ws_msg) = ws_msg else {
            tracing::warn!("({}) connection closed unexpectedly!", &self.address);
            break;
          };

          let ws_msg = match ws_msg {
            Ok(msg) => WsMsgType::from(msg),
            Err(err) => {
              tracing::warn!("({}) error decoding websocket message: {:?}!", &self.address, &err);
              self.forward(err).await;
              break;
            }
          };

          match ws_msg {
            WsMsgType::Text(text) => self.handle_text(text).await,
            WsMsgType::Binary(payload) => self.handle_binary(payload).await,
            WsMsgType::Ping(payload) => self.handle_ping(payload).await,
            WsMsgType::Pong(payload) => self.handle_pong(payload).await,
            WsMsgType::Close(code, reason) => {
              self.handle_closed(code, reason).await;
              break;
            }
          };
        }

        Some(server_msg) = self.rx.recv() => {
          tracing::trace!("(outgoing: {}) sending message: {:?}", &self.address, server_msg);
          self.send(server_msg).await;
        }

        _ = self.cancel_token.cancelled() => {
          tracing::debug!("({}) connection was cancelled, shutting down", &self.address);
          self.close().await;
          break;
        }
      }
    }
  }

  async fn handle_text(&mut self, text: String) {
    tracing::trace!("(incoming: {}) new message: {}", &self.address, &text);
    let msg = match serde_json::from_str::<PossibleRecvMsg>(&text) {
      Ok(msg) => msg,
      error => {
        return tracing::warn!(
          "({}) failed to deserialize incoming message!! message: {text}; error: {error:?}",
          &self.address
        );
      }
    };

    if self.mode != ClientMode::Stock
      && (matches!(msg, PossibleRecvMsg::Stock(_)) || matches!(msg, PossibleRecvMsg::StockInterApp { .. }))
    {
      tracing::warn!(
        "({}) received a stock message, falling back to stock mode...",
        &self.address
      );
      self.mode = ClientMode::Stock;

      self.forward(ForwardMsg::ChangeMode(ClientMode::Stock)).await;
    };

    tracing::trace!("(incoming: {}) decoded message: {:?}", &self.address, &msg);
    self.forward(msg).await;
  }

  async fn handle_binary(&self, payload: tokio_websockets::Payload) {
    tracing::trace!("({}) binary data received? payload: {:?}", &self.address, payload);
  }

  async fn handle_pong(&self, payload: tokio_websockets::Payload) {
    tracing::trace!("({}) pong received? payload: {:?}", &self.address, payload);
  }

  async fn forward(&self, data: impl Into<ForwardMsg>) {
    let data = data.into();

    if let Err(err) = self.tx.send((self.address, data).into()).await {
      tracing::error!("({}) error sending message to connman: {:?}", &self.address, err);
    };
  }

  async fn send(&mut self, msg: PossibleSendMsg) {
    let json = match serde_json::to_string(&msg) {
      Ok(json) => json,
      Err(err) => {
        return tracing::error!(target: "bridgething::ws::connection::send", "({}) error converting message to json!!: {:?}", &self.address, err);
      }
    };
    tracing::trace!(target: "bridgething::ws::connection::send", "sending json: {:?}", json);

    if let Err(err) = self.stream.send(tokio_websockets::Message::text(json)).await {
      tracing::error!(target: "bridgething::ws::connection::send", "({}) error sending message to websocket!!: {:?}", &self.address, err);
    };
  }

  async fn close(&mut self) {
    if let Err(err) = self
      .stream
      .send(tokio_websockets::Message::close(
        Some(tokio_websockets::CloseCode::NORMAL_CLOSURE),
        "bye",
      ))
      .await
    {
      tracing::error!("({}) error sending message to websocket!!: {:?}", &self.address, err);
    };
  }

  async fn handle_closed(&self, code: tokio_websockets::CloseCode, reason: String) {
    tracing::info!(
      "connection from {} closed with code {:?} and reason {}",
      &self.address,
      code,
      &reason
    );
    self.forward((code, reason.to_owned())).await;
  }

  async fn handle_ping(&mut self, payload: tokio_websockets::Payload) {
    if let Err(err) = self
      .stream
      .send(tokio_websockets::Message::pong(payload.to_owned()))
      .await
    {
      tracing::error!("({}) error sending message to websocket: {:?}", &self.address, err);
    };
  }
}

enum WsMsgType {
  Text(String),
  Binary(tokio_websockets::Payload),
  Pong(tokio_websockets::Payload),
  Ping(tokio_websockets::Payload),
  Close(tokio_websockets::CloseCode, String),
}

impl From<tokio_websockets::Message> for WsMsgType {
  fn from(msg: tokio_websockets::Message) -> Self {
    if msg.is_text() {
      Self::Text(
        msg
          .as_text()
          .expect("this message said it was text. this should never fail.")
          .to_owned(),
      )
    } else if msg.is_pong() {
      Self::Pong(msg.into_payload())
    } else if msg.is_ping() {
      Self::Ping(msg.into_payload())
    } else if msg.is_close() {
      let (code, reason) = msg
        .as_close()
        .expect("this message said it was a close message. this should never fail.");
      Self::Close(code, reason.to_owned())
    } else {
      Self::Binary(msg.into_payload())
    }
  }
}

#[derive(Debug)]
enum ForwardMsg {
  Msg(Uuid, PossibleRecvMsg),
  ConnectionClosed(tokio_websockets::CloseCode, String),
  Error(tokio_websockets::Error),
  ChangeMode(ClientMode),
}

impl From<(tokio_websockets::CloseCode, String)> for ForwardMsg {
  fn from((close_code, msg): (tokio_websockets::CloseCode, String)) -> Self {
    Self::ConnectionClosed(close_code, msg)
  }
}

impl From<tokio_websockets::Error> for ForwardMsg {
  fn from(err: tokio_websockets::Error) -> Self {
    Self::Error(err)
  }
}

impl From<PossibleRecvMsg> for ForwardMsg {
  fn from(msg: PossibleRecvMsg) -> Self {
    ForwardMsg::Msg(msg.uuid(), msg)
  }
}

impl From<(SocketAddr, ForwardMsg)> for RecvMsg {
  fn from((from, fwd): (SocketAddr, ForwardMsg)) -> Self {
    match fwd {
      ForwardMsg::Msg(id, data) => {
        let stock_msg_id = if let PossibleRecvMsg::StockInterApp { msg_id, .. } = data {
          Some(msg_id)
        } else {
          None
        };

        Self {
          id,
          from,
          data: data.into(),
          stock_msg_id,
        }
      }
      ForwardMsg::ConnectionClosed(close_code, msg) => Self {
        id: Uuid::now_v7(),
        from,
        data: RecvMsgData::ConnectionClosed(close_code, msg),
        stock_msg_id: None,
      },
      ForwardMsg::Error(err) => Self {
        id: Uuid::now_v7(),
        from,
        data: RecvMsgData::Error(err),
        stock_msg_id: None,
      },
      ForwardMsg::ChangeMode(mode) => Self {
        id: Uuid::now_v7(),
        from,
        data: RecvMsgData::ChangeMode(mode),
        stock_msg_id: None,
      },
    }
  }
}
