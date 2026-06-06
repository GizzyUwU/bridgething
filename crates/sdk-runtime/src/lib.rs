//! Transport-agnostic runtime shared by the Rust bridgething SDKs
//! (`bridgething-client`, `bridgething-gateway`). Provides the generic
//! [`Connection`] driver over the [`Protocol`] trait: typed `command` /
//! `event` / `request` built on libbridgething's `WireCommand` /
//! `WireEvent` / `WireRequest` marker traits, plus an inbound event
//! broadcast. Each SDK crate supplies a `Protocol` impl and concrete
//! [`Connector`] transports; the named ergonomic surface is layered on
//! top by codegen.

mod connection;
mod error;
mod handle;
mod protocol;
mod transport;

pub use connection::Connection;
pub use error::{RequestFailure, SdkError, TransportError};
pub use handle::MsgHandle;
pub use protocol::Protocol;
pub use transport::{Connector, InboundHalf, OutboundHalf};

#[cfg(test)]
mod tests {
  use libbridgething::{
    protocol::PrioritizedFrame,
    wire::{MsgMeta, RequestError, ResponseMeta, WireRequest},
  };
  use tokio::sync::mpsc;
  use uuid::Uuid;

  use super::*;

  #[derive(Clone)]
  struct Msg {
    id: Uuid,
    meta: MsgMeta,
    data: String,
  }

  struct FakeProto;
  impl Protocol for FakeProto {
    type OutData = String;
    type InData = String;
    type OutMsg = Msg;
    type InMsg = Msg;
    fn envelope(id: Uuid, meta: MsgMeta, data: String) -> Msg {
      Msg { id, meta, data }
    }
    fn in_id(msg: &Msg) -> Uuid {
      msg.id
    }
    fn in_meta(msg: &Msg) -> &MsgMeta {
      &msg.meta
    }
    fn in_data(msg: Msg) -> String {
      msg.data
    }
  }

  struct Echo(String);
  impl From<Echo> for String {
    fn from(e: Echo) -> String {
      e.0
    }
  }
  impl WireRequest for Echo {
    type Outbound = String;
    type Inbound = String;
    type Response = String;
    type DomainError = ();
    fn extract(data: String) -> Result<String, RequestError<()>> {
      Ok(data)
    }
    fn encode_response(r: String) -> String {
      r
    }
    fn encode_domain_error(_: ()) -> String {
      String::new()
    }
  }

  struct ChanConnector {
    out: mpsc::UnboundedSender<Msg>,
    inn: mpsc::UnboundedReceiver<Msg>,
  }
  struct ChanOut(mpsc::UnboundedSender<Msg>);
  struct ChanIn(mpsc::UnboundedReceiver<Msg>);

  impl Connector<FakeProto> for ChanConnector {
    type Out = ChanOut;
    type In = ChanIn;
    fn split(self) -> (ChanOut, ChanIn) {
      (ChanOut(self.out), ChanIn(self.inn))
    }
  }
  impl OutboundHalf<FakeProto> for ChanOut {
    async fn send(&mut self, frame: PrioritizedFrame<Msg>) -> Result<(), TransportError> {
      self.0.send(frame.msg).map_err(|_| TransportError::Closed)
    }
  }
  impl InboundHalf<FakeProto> for ChanIn {
    async fn recv(&mut self) -> Option<Result<Msg, TransportError>> {
      self.0.recv().await.map(Ok)
    }
  }

  fn far_end(mut from_client: mpsc::UnboundedReceiver<Msg>, to_client: mpsc::UnboundedSender<Msg>) {
    tokio::spawn(async move {
      while let Some(msg) = from_client.recv().await {
        match msg.meta {
          MsgMeta::Request => {
            let _ = to_client.send(Msg {
              id: Uuid::now_v7(),
              meta: MsgMeta::Response(ResponseMeta { request_id: msg.id }),
              data: format!("echo:{}", msg.data),
            });
          }
          MsgMeta::Event | MsgMeta::Command => {
            let _ = to_client.send(Msg {
              id: Uuid::now_v7(),
              meta: MsgMeta::Event,
              data: format!("seen:{}", msg.data),
            });
          }
          MsgMeta::Response(_) => {}
        }
      }
    });
  }

  fn connect() -> Connection<FakeProto> {
    let (c_out, fe_in) = mpsc::unbounded_channel();
    let (fe_out, c_in) = mpsc::unbounded_channel();
    far_end(fe_in, fe_out);
    Connection::spawn(ChanConnector { out: c_out, inn: c_in })
  }

  #[tokio::test]
  async fn request_correlates_response() {
    let conn = connect();
    let resp = conn.request(Echo("hi".into())).await.expect("response");
    assert_eq!(resp, "echo:hi");
  }

  #[tokio::test]
  async fn event_reaches_far_end_and_broadcasts_back() {
    let conn = connect();
    let mut events = conn.events();
    conn
      .send_data(MsgMeta::Event, "ping".into(), libbridgething::Priority::Normal)
      .await
      .unwrap();
    let got = tokio::time::timeout(std::time::Duration::from_secs(1), events.recv())
      .await
      .expect("not timed out")
      .expect("event");
    assert_eq!(got.data, "seen:ping");
  }

  #[tokio::test]
  async fn handle_answers_inbound_request() {
    // far end sends us a request; we build a handle and respond_to it.
    let (c_out, mut fe_in) = mpsc::unbounded_channel();
    let (fe_out, c_in) = mpsc::unbounded_channel();
    let conn = Connection::spawn(ChanConnector { out: c_out, inn: c_in });
    let mut inbound = conn.events();

    let req_id = Uuid::now_v7();
    fe_out
      .send(Msg {
        id: req_id,
        meta: MsgMeta::Request,
        data: "ping".into(),
      })
      .unwrap();

    let msg = inbound.recv().await.expect("inbound request");
    assert!(matches!(msg.meta, MsgMeta::Request));
    conn
      .handle(&msg)
      .respond_to::<Echo>("pong".into())
      .await
      .expect("respond");

    let resp = tokio::time::timeout(std::time::Duration::from_secs(1), fe_in.recv())
      .await
      .expect("not timed out")
      .expect("a response");
    assert!(matches!(resp.meta, MsgMeta::Response(ResponseMeta { request_id }) if request_id == req_id));
    assert_eq!(resp.data, "pong");
  }

  #[tokio::test]
  async fn normal_preempts_queued_lower_lanes() {
    use std::sync::{Arc, Mutex};

    use libbridgething::Priority;

    struct GatedConnector {
      sent: Arc<Mutex<Vec<(Priority, String)>>>,
      gate: Arc<tokio::sync::Semaphore>,
      inn: mpsc::UnboundedReceiver<Msg>,
    }
    struct GatedOut {
      sent: Arc<Mutex<Vec<(Priority, String)>>>,
      gate: Arc<tokio::sync::Semaphore>,
    }
    impl Connector<FakeProto> for GatedConnector {
      type Out = GatedOut;
      type In = ChanIn;
      fn split(self) -> (GatedOut, ChanIn) {
        (
          GatedOut {
            sent: self.sent,
            gate: self.gate,
          },
          ChanIn(self.inn),
        )
      }
    }
    impl OutboundHalf<FakeProto> for GatedOut {
      async fn send(&mut self, frame: PrioritizedFrame<Msg>) -> Result<(), TransportError> {
        self.gate.acquire().await.expect("gate open").forget();
        self.sent.lock().unwrap().push((frame.priority, frame.msg.data));
        Ok(())
      }
    }

    let sent = Arc::new(Mutex::new(Vec::new()));
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let (_fe_out, c_in) = mpsc::unbounded_channel();
    let conn = Connection::spawn(GatedConnector {
      sent: sent.clone(),
      gate: gate.clone(),
      inn: c_in,
    });

    let send = |conn: Connection<FakeProto>, data: &str, priority: Priority| {
      let data = data.to_string();
      tokio::spawn(async move { conn.send_data(MsgMeta::Event, data, priority).await })
    };

    // bg0 dequeues first and blocks inside the gated transport send.
    let t0 = send(conn.clone(), "bg0", Priority::Background);
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    // these queue behind the in-flight send, in arrival order...
    let t1 = send(conn.clone(), "bg1", Priority::Background);
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let t2 = send(conn.clone(), "bulk0", Priority::Bulk);
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let t3 = send(conn.clone(), "norm0", Priority::Normal);
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // ...and drain in lane order once the transport opens up.
    gate.add_permits(4);
    for t in [t0, t1, t2, t3] {
      t.await.unwrap().unwrap();
    }

    let order: Vec<String> = sent.lock().unwrap().iter().map(|(_, d)| d.clone()).collect();
    assert_eq!(order, vec!["bg0", "norm0", "bulk0", "bg1"]);
  }

  #[tokio::test]
  async fn request_times_out_when_no_response() {
    let (c_out, _fe_in) = mpsc::unbounded_channel();
    let (_fe_out, c_in) = mpsc::unbounded_channel();
    let conn =
      Connection::spawn(ChanConnector { out: c_out, inn: c_in }).with_timeout(std::time::Duration::from_millis(50));
    let err = conn.request(Echo("x".into())).await.expect_err("should time out");
    assert!(matches!(err, RequestFailure::Timeout));
  }
}
