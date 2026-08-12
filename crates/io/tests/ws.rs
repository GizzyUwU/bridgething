#![cfg(feature = "native-io")]
#![allow(clippy::result_large_err)]

use std::{sync::Arc, time::Duration};

use bridgething_io::{HttpHeader, TungsteniteTransport, WsConnect, WsEvent, WsFrame, WsInbox, WsTransport};
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{
  Message,
  handshake::server::{Request, Response},
};
use uuid::Uuid;

async fn echo_server() -> u16 {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a free port");
  let port = listener.local_addr().expect("a bound address").port();
  tokio::spawn(async move {
    while let Ok((socket, _)) = listener.accept().await {
      tokio::spawn(async move {
        let mut seen_protocol = None;
        let mut seen_header = None;
        let accepted = tokio_tungstenite::accept_hdr_async(socket, |req: &Request, mut response: Response| {
          seen_protocol = req
            .headers()
            .get("sec-websocket-protocol")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
          seen_header = req
            .headers()
            .get("x-probe")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
          if let Some(protocol) = seen_protocol.as_deref().and_then(|list| list.split(',').next()) {
            response.headers_mut().insert(
              "sec-websocket-protocol",
              protocol.trim().parse().expect("a header value"),
            );
          }
          Ok(response)
        })
        .await;
        let Ok(mut ws) = accepted else { return };
        let probe = seen_header.unwrap_or_default();
        if !probe.is_empty() {
          let _ = ws.send(Message::Text(format!("probe:{probe}").into())).await;
        }
        while let Some(Ok(message)) = ws.next().await {
          match message {
            Message::Text(_) | Message::Binary(_) => {
              if ws.send(message).await.is_err() {
                return;
              }
            }
            Message::Close(_) => return,
            _ => {}
          }
        }
      });
    }
  });
  port
}

fn connect_to(port: u16, id: Uuid) -> WsConnect {
  WsConnect {
    id,
    url: format!("ws://127.0.0.1:{port}"),
    protocols: Vec::new(),
    headers: Vec::new(),
  }
}

async fn next_event(rx: &mut mpsc::UnboundedReceiver<WsEvent>) -> WsEvent {
  tokio::time::timeout(Duration::from_secs(5), rx.recv())
    .await
    .expect("the transport went quiet")
    .expect("the inbox closed")
}

#[tokio::test(flavor = "multi_thread")]
async fn several_sockets_share_one_inbox_without_crossing_their_frames() {
  let port = echo_server().await;
  let transport = TungsteniteTransport::new();
  let (tx, mut rx) = mpsc::unbounded_channel();
  let inbox = Arc::new(WsInbox::new(tx));
  let first = Uuid::from_u128(1);
  let second = Uuid::from_u128(2);

  transport.connect(connect_to(port, first), inbox.clone());
  transport.connect(connect_to(port, second), inbox);
  for _ in 0..2 {
    assert!(matches!(next_event(&mut rx).await, WsEvent::Open { .. }));
  }

  transport.send(first, WsFrame::Text("one".to_string()));
  transport.send(second, WsFrame::Binary(vec![0xde, 0xad]));

  let mut seen: Vec<(Uuid, WsFrame)> = Vec::new();
  while seen.len() < 2 {
    if let WsEvent::Frame { id, frame } = next_event(&mut rx).await {
      seen.push((id, frame));
    }
  }
  seen.sort_by_key(|(id, _)| *id);

  assert_eq!(seen[0].0, first);
  assert_eq!(seen[0].1, WsFrame::Text("one".to_string()));
  assert_eq!(seen[1].0, second);
  assert_eq!(
    seen[1].1,
    WsFrame::Binary(vec![0xde, 0xad]),
    "a binary frame must survive the seam as binary"
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_disconnect_reports_the_close_it_sent() {
  let port = echo_server().await;
  let transport = TungsteniteTransport::new();
  let (tx, mut rx) = mpsc::unbounded_channel();
  let id = Uuid::from_u128(3);

  transport.connect(connect_to(port, id), Arc::new(WsInbox::new(tx)));
  assert!(matches!(next_event(&mut rx).await, WsEvent::Open { .. }));

  transport.disconnect(id, Some(1001), Some("going away".to_string()));

  match next_event(&mut rx).await {
    WsEvent::Closed {
      id: closed,
      code,
      reason,
    } => {
      assert_eq!(closed, id);
      assert_eq!(code, Some(1001));
      assert_eq!(reason, "going away");
    }
    other => panic!("expected a close, got {other:?}"),
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_handshake_carries_the_subprotocol_and_headers_it_was_given() {
  let port = echo_server().await;
  let transport = TungsteniteTransport::new();
  let (tx, mut rx) = mpsc::unbounded_channel();
  let id = Uuid::from_u128(4);

  transport.connect(
    WsConnect {
      id,
      url: format!("ws://127.0.0.1:{port}"),
      protocols: vec!["chat".to_string()],
      headers: vec![HttpHeader {
        name: "x-probe".to_string(),
        value: "seen".to_string(),
      }],
    },
    Arc::new(WsInbox::new(tx)),
  );

  match next_event(&mut rx).await {
    WsEvent::Open { accepted_protocol, .. } => assert_eq!(
      accepted_protocol.as_deref(),
      Some("chat"),
      "the subprotocol the server picked is reported rather than guessed"
    ),
    other => panic!("expected an open, got {other:?}"),
  }
  match next_event(&mut rx).await {
    WsEvent::Frame {
      frame: WsFrame::Text(text),
      ..
    } => assert_eq!(text, "probe:seen", "the handshake headers reached the server"),
    other => panic!("expected the probe frame, got {other:?}"),
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_handshake_that_never_answers_reports_a_close_rather_than_hanging() {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a free port");
  let port = listener.local_addr().expect("a bound address").port();
  tokio::spawn(async move {
    let mut accepted = Vec::new();
    while let Ok((socket, _)) = listener.accept().await {
      accepted.push(socket);
    }
  });
  let transport = TungsteniteTransport::with_connect_timeout(Duration::from_millis(150));
  let (tx, mut rx) = mpsc::unbounded_channel();
  let id = Uuid::from_u128(6);

  transport.connect(connect_to(port, id), Arc::new(WsInbox::new(tx)));

  match next_event(&mut rx).await {
    WsEvent::Closed { id: closed, reason, .. } => {
      assert_eq!(closed, id);
      assert!(reason.contains("connect timed out"), "got {reason}");
    }
    other => panic!("expected a close, got {other:?}"),
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connect_to_nothing_reports_a_close_rather_than_hanging() {
  let dead = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a free port");
  let port = dead.local_addr().expect("a bound address").port();
  drop(dead);
  let transport = TungsteniteTransport::new();
  let (tx, mut rx) = mpsc::unbounded_channel();
  let id = Uuid::from_u128(5);

  transport.connect(connect_to(port, id), Arc::new(WsInbox::new(tx)));

  match next_event(&mut rx).await {
    WsEvent::Closed { id: closed, reason, .. } => {
      assert_eq!(closed, id);
      assert!(reason.contains("connect failed"), "got {reason}");
    }
    other => panic!("expected a close, got {other:?}"),
  }
}
