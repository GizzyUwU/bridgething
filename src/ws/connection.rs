use futures::{SinkExt, StreamExt};
use std::net::SocketAddr;
use tokio::{net::TcpStream, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tokio_websockets::WebSocketStream;

use crate::msg::{AddressedRecvMessage, RecvMessage, RecvMessageWithMeta, RecvTx, SendMessage, SendRx};

pub struct Connection {
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
    tracing::debug!("spawning listener for {address}");
    tokio::spawn(async move {
      Self {
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
            tracing::warn!("({}) connection ", &self.address);
            break;
          };

          let ws_msg = match ws_msg {
            Ok(msg) => WsMsgType::from(msg),
            Err(err) => {
              self.forward(RecvMessageWithMeta::Error(err)).await;
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
          tracing::trace!("({}) sending message: {:?}", &self.address, server_msg);
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

  async fn handle_text(&self, text: String) {
    tracing::trace!("({}) new text message: {}", &self.address, &text);
    let Ok(msg) = serde_json::from_str::<RecvMessage>(&text) else {
      return tracing::warn!(
        "({}) failed to deserialize incoming message!! message: {text}",
        self.address
      );
    };

    tracing::trace!("({}) decoded message: {:?}", &self.address, &msg);
    self.forward(msg.into()).await;
  }

  async fn handle_binary(&self, payload: tokio_websockets::Payload) {
    tracing::trace!("({}) binary data received? payload: {:?}", &self.address, payload);
  }

  async fn handle_pong(&self, payload: tokio_websockets::Payload) {
    tracing::trace!("({}) pong received? payload: {:?}", &self.address, payload);
  }

  async fn forward(&self, data: RecvMessageWithMeta) {
    if let Err(err) = self
      .tx
      .send(AddressedRecvMessage {
        from: self.address,
        data,
      })
      .await
    {
      tracing::error!("({}) error sending message to connman: {:?}", &self.address, err);
    };
  }

  async fn send(&mut self, msg: SendMessage) {
    let json = match serde_json::to_string(&msg) {
      Ok(json) => json,
      Err(err) => return tracing::error!("({}) error converting message to json!!: {:?}", &self.address, err),
    };

    if let Err(err) = self.stream.send(tokio_websockets::Message::text(json)).await {
      tracing::error!("({}) error sending message to websocket!!: {:?}", &self.address, err);
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
    self
      .forward(RecvMessageWithMeta::ConnectionClosed(code, reason.to_owned()))
      .await;
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
