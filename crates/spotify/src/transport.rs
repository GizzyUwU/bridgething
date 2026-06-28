//! credit to the librespot project

use std::sync::{Arc, Mutex};

use futures::{SinkExt, StreamExt};
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};

pub(crate) enum WsEvent {
  Open,
  Text(String),
  Closed(String),
}

#[derive(uniffi::Object)]
pub struct WsInbox {
  tx: mpsc::UnboundedSender<WsEvent>,
}

impl WsInbox {
  pub(crate) fn new(tx: mpsc::UnboundedSender<WsEvent>) -> Self {
    WsInbox { tx }
  }
}

#[uniffi::export]
impl WsInbox {
  pub fn on_open(&self) {
    tracing::debug!("ws: open");
    let _ = self.tx.send(WsEvent::Open);
  }

  pub fn on_text(&self, text: String) {
    tracing::trace!(frame = %text, "ws: recv");
    let _ = self.tx.send(WsEvent::Text(text));
  }

  pub fn on_closed(&self, reason: String) {
    tracing::debug!(%reason, "ws: closed");
    let _ = self.tx.send(WsEvent::Closed(reason));
  }
}

#[uniffi::export(with_foreign)]
pub trait WsTransport: Send + Sync {
  fn connect(&self, url: String, inbox: Arc<WsInbox>);
  fn send_text(&self, text: String);
  fn disconnect(&self);
}

#[derive(Default)]
pub struct TungsteniteTransport {
  conn: Mutex<Option<Conn>>,
}

struct Conn {
  out: mpsc::UnboundedSender<String>,
  task: JoinHandle<()>,
}

impl TungsteniteTransport {
  pub fn new() -> Self {
    Self::default()
  }
}

impl WsTransport for TungsteniteTransport {
  fn connect(&self, url: String, inbox: Arc<WsInbox>) {
    tracing::debug!("ws: connecting (native transport)");
    let (out, out_rx) = mpsc::unbounded_channel::<String>();
    let task = tokio::spawn(run(url, inbox, out_rx));
    if let Some(prev) = self.conn.lock().unwrap().replace(Conn { out, task }) {
      prev.task.abort();
    }
  }

  fn send_text(&self, text: String) {
    tracing::trace!(frame = %text, "ws: send");
    if let Some(conn) = self.conn.lock().unwrap().as_ref() {
      let _ = conn.out.send(text);
    }
  }

  fn disconnect(&self) {
    tracing::debug!("ws: disconnect (native transport)");
    if let Some(conn) = self.conn.lock().unwrap().take() {
      conn.task.abort();
    }
  }
}

async fn run(url: String, inbox: Arc<WsInbox>, mut out_rx: mpsc::UnboundedReceiver<String>) {
  let ws = match connect_async(url.as_str()).await {
    Ok((ws, _resp)) => ws,
    Err(e) => {
      inbox.on_closed(format!("connect failed: {e}"));
      return;
    }
  };
  inbox.on_open();
  let (mut sink, mut stream) = ws.split();
  loop {
    tokio::select! {
      msg = stream.next() => match msg {
        Some(Ok(WsMessage::Text(t))) => inbox.on_text(t.to_string()),
        Some(Ok(WsMessage::Ping(p))) => {
          if sink.send(WsMessage::Pong(p)).await.is_err() {
            inbox.on_closed("write error".to_string());
            return;
          }
        }
        Some(Ok(WsMessage::Close(_))) | None => {
          inbox.on_closed("closed".to_string());
          return;
        }
        Some(Ok(_)) => {}
        Some(Err(e)) => {
          inbox.on_closed(format!("read error: {e}"));
          return;
        }
      },
      out = out_rx.recv() => match out {
        Some(text) => {
          if sink.send(WsMessage::Text(text.into())).await.is_err() {
            inbox.on_closed("write error".to_string());
            return;
          }
        }
        None => return,
      }
    }
  }
}
