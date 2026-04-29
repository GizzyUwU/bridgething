use axum::{
  body::Bytes,
  extract::ws::{self, Utf8Bytes, WebSocket},
};
use futures::{
  SinkExt, StreamExt,
  stream::{SplitSink, SplitStream},
};
use std::net::SocketAddr;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::handler::client::{ClientMode, PossibleRecvMsg, PossibleSendMsg, RecvMsg, RecvMsgData, RecvTx, SendRx};

pub struct Connection {
  mode: ClientMode,
  address: SocketAddr,

  writer: SplitSink<WebSocket, ws::Message>,
  reader: SplitStream<WebSocket>,

  tx: RecvTx,
  rx: SendRx,

  cancel_token: CancellationToken,
}

impl Connection {
  pub fn spawn(
    address: SocketAddr,
    ws: WebSocket,
    tx: RecvTx,
    rx: SendRx,
    cancel_token: CancellationToken,
    mode: ClientMode,
  ) -> JoinHandle<()> {
    tracing::debug!("spawning listener for {address} in {:?} mode", &mode);
    let (writer, reader) = ws.split();

    tokio::spawn(async move {
      Self {
        mode,
        address,

        writer,
        reader,

        tx,
        rx,

        cancel_token,
      }
      .listen()
      .await
    })
  }

  pub async fn listen(&mut self) {
    loop {
      tokio::select! {
        ws_msg = self.reader.next() => {
          let Some(ws_msg) = ws_msg else {
            tracing::warn!("({}) connection closed unexpectedly!", &self.address);
            break;
          };

          let ws_msg = match ws_msg {
            Ok(msg) => msg,
            Err(err) => {
              tracing::warn!("({}) error decoding websocket message: {:?}!", &self.address, &err);
              self.forward(err).await;
              break;
            }
          };

          match ws_msg {
            ws::Message::Text(text) => self.handle_text(text).await,
            ws::Message::Binary(payload) => self.handle_binary(payload).await,
            ws::Message::Ping(payload) => self.handle_ping(payload).await,
            ws::Message::Pong(payload) => self.handle_pong(payload).await,
            ws::Message::Close(frame) => {
              self.handle_closed(frame).await;
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

  async fn handle_text(&mut self, text: Utf8Bytes) {
    tracing::trace!("(incoming: {}) new message: {}", &self.address, text.as_str());
    let msg = match serde_json::from_str::<PossibleRecvMsg>(text.as_str()) {
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

  async fn handle_binary(&self, payload: Bytes) {
    tracing::trace!("({}) binary data received? payload: {:?}", &self.address, payload);
  }

  async fn handle_pong(&self, payload: Bytes) {
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

    if let Err(err) = self.writer.send(ws::Message::Text(json.into())).await {
      tracing::error!(target: "bridgething::ws::connection::send", "({}) error sending message to websocket!!: {:?}", &self.address, err);
    };
  }

  async fn close(&mut self) {
    if let Err(err) = self
      .writer
      .send(ws::Message::Close(Some(ws::CloseFrame {
        code: ws::close_code::NORMAL,
        reason: "bye".into(),
      })))
      .await
    {
      tracing::error!("({}) error sending message to websocket!!: {:?}", &self.address, err);
    };
  }

  async fn handle_closed(&self, frame: Option<ws::CloseFrame>) {
    tracing::info!("connection from {} closed with frame {:?}", &self.address, frame);
    if let Some(frame) = frame {
      self.forward((frame.code, frame.reason.as_str().to_string())).await;
    } else {
      self
        .forward((ws::close_code::ABNORMAL, "no close frame".to_string()))
        .await;
    }
  }

  async fn handle_ping(&mut self, payload: Bytes) {
    if let Err(err) = self.writer.send(ws::Message::Pong(payload)).await {
      tracing::error!("({}) error sending message to websocket: {:?}", &self.address, err);
    };
  }
}

#[derive(Debug)]
enum ForwardMsg {
  Msg(Uuid, PossibleRecvMsg),
  ConnectionClosed(ws::CloseCode, String),
  Error(axum::Error),
  ChangeMode(ClientMode),
}

impl From<(ws::CloseCode, String)> for ForwardMsg {
  fn from((close_code, msg): (ws::CloseCode, String)) -> Self {
    Self::ConnectionClosed(close_code, msg)
  }
}

impl From<axum::Error> for ForwardMsg {
  fn from(err: axum::Error) -> Self {
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
