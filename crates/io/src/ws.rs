use std::sync::Arc;

use tokio::sync::mpsc;
use uuid::Uuid;

use crate::http::HttpHeader;

#[derive(Debug, Clone)]
pub struct WsConnect {
  pub id: Uuid,
  pub url: String,
  pub protocols: Vec<String>,
  pub headers: Vec<HttpHeader>,
}

#[derive(Clone, PartialEq, Eq)]
pub enum WsFrame {
  Text(String),
  Binary(Vec<u8>),
}

impl std::fmt::Debug for WsFrame {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Text(text) => write!(f, "Text({} chars)", text.len()),
      Self::Binary(bytes) => write!(f, "Binary({} bytes)", bytes.len()),
    }
  }
}

#[derive(Debug)]
pub enum WsEvent {
  Open {
    id: Uuid,
    accepted_protocol: Option<String>,
  },
  Frame {
    id: Uuid,
    frame: WsFrame,
  },
  Closed {
    id: Uuid,
    code: Option<u16>,
    reason: String,
  },
}

impl WsEvent {
  pub fn connection(&self) -> Uuid {
    match self {
      Self::Open { id, .. } | Self::Frame { id, .. } | Self::Closed { id, .. } => *id,
    }
  }
}

pub struct WsInbox {
  tx: mpsc::UnboundedSender<WsEvent>,
}

impl WsInbox {
  pub fn new(tx: mpsc::UnboundedSender<WsEvent>) -> Self {
    WsInbox { tx }
  }

  pub fn on_open(&self, id: Uuid, accepted_protocol: Option<String>) {
    tracing::debug!(%id, ?accepted_protocol, "ws: open");
    let _ = self.tx.send(WsEvent::Open { id, accepted_protocol });
  }

  pub fn on_text(&self, id: Uuid, text: String) {
    tracing::trace!(%id, frame = %text, "ws: recv");
    let _ = self.tx.send(WsEvent::Frame {
      id,
      frame: WsFrame::Text(text),
    });
  }

  pub fn on_binary(&self, id: Uuid, bytes: Vec<u8>) {
    tracing::trace!(%id, len = bytes.len(), "ws: recv binary");
    let _ = self.tx.send(WsEvent::Frame {
      id,
      frame: WsFrame::Binary(bytes),
    });
  }

  pub fn on_closed(&self, id: Uuid, code: Option<u16>, reason: String) {
    tracing::debug!(%id, ?code, %reason, "ws: closed");
    let _ = self.tx.send(WsEvent::Closed { id, code, reason });
  }
}

pub trait WsTransport: Send + Sync {
  fn connect(&self, connect: WsConnect, inbox: Arc<WsInbox>);
  fn send(&self, id: Uuid, frame: WsFrame);
  fn disconnect(&self, id: Uuid, code: Option<u16>, reason: Option<String>);
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_inbox_forwards_events_in_arrival_order() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let inbox = WsInbox::new(tx);
    let id = Uuid::from_u128(1);

    inbox.on_open(id, None);
    inbox.on_text(id, "hello".to_string());
    inbox.on_closed(id, Some(1000), "bye".to_string());

    assert!(matches!(rx.try_recv().unwrap(), WsEvent::Open { .. }));
    assert!(matches!(rx.try_recv().unwrap(), WsEvent::Frame { frame: WsFrame::Text(t), .. } if t == "hello"));
    assert!(matches!(
      rx.try_recv().unwrap(),
      WsEvent::Closed { code: Some(1000), .. }
    ));
  }

  #[test]
  fn every_event_names_the_connection_it_came_from() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let inbox = WsInbox::new(tx);
    let first = Uuid::from_u128(1);
    let second = Uuid::from_u128(2);

    inbox.on_text(first, "one".to_string());
    inbox.on_binary(second, vec![0x01, 0x02]);
    inbox.on_text(first, "two".to_string());

    let seen: Vec<Uuid> = std::iter::from_fn(|| rx.try_recv().ok())
      .map(|event| event.connection())
      .collect();
    assert_eq!(seen, [first, second, first]);
  }

  #[test]
  fn a_closed_receiver_does_not_panic_the_reporter() {
    let (tx, rx) = mpsc::unbounded_channel();
    let inbox = WsInbox::new(tx);
    drop(rx);

    inbox.on_text(Uuid::from_u128(1), "dropped".to_string());
  }
}
