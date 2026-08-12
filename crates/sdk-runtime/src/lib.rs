mod connection;
mod error;
mod handle;
#[cfg(any(test, feature = "lane-corpus"))]
pub mod lane_corpus;
#[cfg(test)]
mod lane_trace;
mod protocol;
mod reply;
pub mod rt;
pub mod sched;
mod transport;

pub use connection::{Connection, LaneLimits};
pub use error::{RequestFailure, SdkError, TransportError};
pub use handle::MsgHandle;
pub use protocol::Protocol;
pub use reply::{AfterResponse, Reply};
pub use sched::{Batch, LaneFeed, LaneItem, LaneScheduler, OutboundLanes, lanes};
pub use transport::{Connector, InboundHalf, OutboundHalf};

#[cfg(test)]
mod tests {
  use std::{
    sync::{Arc, Mutex},
    time::Duration,
  };

  use bytes::Bytes;
  use libbridgething::{
    Priority,
    protocol::PrioritizedFrame,
    wire::{MsgMeta, RequestError, ResponseMeta, WireRequest},
  };
  use serde::{Deserialize, Serialize};
  use tokio::sync::mpsc;
  use uuid::Uuid;

  use super::*;
  use crate::connection::INBOUND_ERROR_LIMIT;

  #[derive(Clone, Serialize, Deserialize)]
  struct Msg {
    id: Uuid,
    meta: MsgMeta,
    data: String,
  }

  fn encode_msg(frame: PrioritizedFrame<Msg>) -> Result<Bytes, TransportError> {
    let body = serde_json::to_vec(&frame.msg).map_err(|e| TransportError::Encode(e.to_string()))?;
    let mut buf = Vec::with_capacity(5 + body.len());
    buf.push(frame.priority.as_byte());
    buf.extend_from_slice(&(body.len() as u32).to_be_bytes());
    buf.extend_from_slice(&body);
    Ok(Bytes::from(buf))
  }

  fn decode_batch(batch: &Bytes) -> Vec<(Priority, Msg)> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while cursor < batch.len() {
      let priority = Priority::from_byte(batch[cursor]);
      let len = u32::from_be_bytes(batch[cursor + 1..cursor + 5].try_into().expect("4 length bytes")) as usize;
      let body = &batch[cursor + 5..cursor + 5 + len];
      out.push((
        priority,
        serde_json::from_slice(body).expect("fixture json round-trips"),
      ));
      cursor += 5 + len;
    }
    out
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
    fn max_batch_bytes(&self) -> usize {
      4096
    }
    fn encode(frame: PrioritizedFrame<Msg>) -> Result<Bytes, TransportError> {
      encode_msg(frame)
    }
    async fn ready(&mut self) -> Result<(), TransportError> {
      Ok(())
    }
    async fn send_batch(&mut self, batch: Bytes) -> Result<(), TransportError> {
      for (_, msg) in decode_batch(&batch) {
        self.0.send(msg).map_err(|_| TransportError::Closed)?;
      }
      Ok(())
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

  type Sent = Arc<Mutex<Vec<(Priority, String)>>>;

  const GATED_BATCH_BYTES: usize = 2048;

  struct GatedConnector {
    sent: Sent,
    gate: Arc<tokio::sync::Semaphore>,
    inn: mpsc::UnboundedReceiver<Msg>,
  }
  struct GatedOut {
    sent: Sent,
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
    fn max_batch_bytes(&self) -> usize {
      GATED_BATCH_BYTES
    }
    fn encode(frame: PrioritizedFrame<Msg>) -> Result<Bytes, TransportError> {
      encode_msg(frame)
    }
    async fn ready(&mut self) -> Result<(), TransportError> {
      Ok(())
    }
    async fn send_batch(&mut self, batch: Bytes) -> Result<(), TransportError> {
      self.gate.acquire().await.expect("gate open").forget();
      let mut sent = self.sent.lock().unwrap();
      for (priority, msg) in decode_batch(&batch) {
        sent.push((priority, msg.data));
      }
      Ok(())
    }
  }

  fn gated() -> (
    Sent,
    Arc<tokio::sync::Semaphore>,
    Connection<FakeProto>,
    mpsc::UnboundedSender<Msg>,
  ) {
    let sent: Sent = Arc::new(Mutex::new(Vec::new()));
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let (fe_out, c_in) = mpsc::unbounded_channel();
    let conn = Connection::spawn(GatedConnector {
      sent: sent.clone(),
      gate: gate.clone(),
      inn: c_in,
    });
    (sent, gate, conn, fe_out)
  }

  fn spawn_send(
    conn: Connection<FakeProto>,
    data: String,
    priority: Priority,
  ) -> tokio::task::JoinHandle<Result<(), SdkError>> {
    tokio::spawn(async move { conn.send_data(MsgMeta::Event, data, priority).await })
  }

  async fn wait_for_sends(sent: &Sent, want: usize) {
    for _ in 0..400 {
      let landed = sent.lock().unwrap().len();
      if landed >= want {
        return;
      }
      tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!(
      "only {} of {want} frames reached the transport",
      sent.lock().unwrap().len()
    );
  }

  #[tokio::test]
  async fn normal_preempts_queued_lower_lanes() {
    let (sent, gate, conn, _fe_out) = gated();

    let send =
      |conn: Connection<FakeProto>, data: &str, priority: Priority| spawn_send(conn, data.to_string(), priority);

    let t0 = send(conn.clone(), "bg0", Priority::Background);
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let t1 = send(conn.clone(), "bg1", Priority::Background);
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let t2 = send(conn.clone(), "bulk0", Priority::Bulk);
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let t3 = send(conn.clone(), "norm0", Priority::Normal);
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    gate.add_permits(4);
    for t in [t0, t1, t2, t3] {
      t.await.unwrap().unwrap();
    }
    wait_for_sends(&sent, 4).await;

    let order: Vec<String> = sent.lock().unwrap().iter().map(|(_, d)| d.clone()).collect();
    assert_eq!(order, vec!["bg0", "norm0", "bulk0", "bg1"]);
  }

  #[tokio::test]
  async fn a_saturated_normal_lane_cannot_starve_the_lanes_below() {
    const FLOOD: usize = 400;
    const WINDOW: usize = 64;

    let (sent, gate, conn, _fe_out) = gated();
    let mut tasks = Vec::new();

    for i in 0..4 {
      tasks.push(spawn_send(conn.clone(), format!("seed{i}"), Priority::Normal));
    }
    tasks.push(spawn_send(conn.clone(), "bulk".into(), Priority::Bulk));
    tasks.push(spawn_send(conn.clone(), "background".into(), Priority::Background));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    for i in 0..FLOOD {
      tasks.push(spawn_send(conn.clone(), format!("n{i}"), Priority::Normal));
    }
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    gate.add_permits(tasks.len());
    for t in tasks {
      t.await.unwrap().unwrap();
    }
    wait_for_sends(&sent, WINDOW).await;

    let window: Vec<String> = sent
      .lock()
      .unwrap()
      .iter()
      .take(WINDOW)
      .map(|(_, d)| d.clone())
      .collect();
    assert!(
      window.iter().any(|d| d == "bulk"),
      "bulk must land a frame within the first {WINDOW} sends despite a saturated normal lane, got {window:?}"
    );
    assert!(
      window.iter().any(|d| d == "background"),
      "background must land a frame within the first {WINDOW} sends despite a saturated normal lane, got {window:?}"
    );
  }

  #[tokio::test]
  async fn a_full_lane_parks_its_sender_until_a_batch_leaves() {
    let sent: Sent = Arc::new(Mutex::new(Vec::new()));
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let (_fe_out, c_in) = mpsc::unbounded_channel();
    let conn = Connection::spawn_with(
      GatedConnector {
        sent: sent.clone(),
        gate: gate.clone(),
        inn: c_in,
      },
      LaneLimits { max_lane_bytes: 1 },
    );

    for id in ["b0", "b1"] {
      spawn_send(conn.clone(), id.into(), Priority::Bulk)
        .await
        .unwrap()
        .unwrap();
    }
    let parked = spawn_send(conn.clone(), "b2".into(), Priority::Bulk);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!parked.is_finished(), "a full lane parks its sender");

    gate.add_permits(3);
    parked.await.unwrap().unwrap();
    wait_for_sends(&sent, 3).await;

    let order: Vec<String> = sent.lock().unwrap().iter().map(|(_, d)| d.clone()).collect();
    assert_eq!(order, vec!["b0", "b1", "b2"]);
  }

  struct FailingIn {
    left: usize,
    recovers: bool,
  }
  impl InboundHalf<FakeProto> for FailingIn {
    async fn recv(&mut self) -> Option<Result<Msg, TransportError>> {
      if self.left == 0 {
        return std::future::pending().await;
      }
      self.left -= 1;
      if self.recovers && self.left.is_multiple_of(2) {
        Some(Ok(Msg {
          id: Uuid::now_v7(),
          meta: MsgMeta::Event,
          data: "recovered".into(),
        }))
      } else {
        Some(Err(TransportError::Decode("garbage on the wire".into())))
      }
    }
  }

  struct FailingConnector {
    out: mpsc::UnboundedSender<Msg>,
    left: usize,
    recovers: bool,
  }
  impl Connector<FakeProto> for FailingConnector {
    type Out = ChanOut;
    type In = FailingIn;
    fn split(self) -> (ChanOut, FailingIn) {
      (
        ChanOut(self.out),
        FailingIn {
          left: self.left,
          recovers: self.recovers,
        },
      )
    }
  }

  async fn link_dies_within(conn: &Connection<FakeProto>, tries: usize) -> bool {
    for _ in 0..tries {
      if matches!(
        conn.send_data(MsgMeta::Event, "probe".into(), Priority::Normal).await,
        Err(SdkError::Disconnected)
      ) {
        return true;
      }
      tokio::time::sleep(Duration::from_millis(5)).await;
    }
    false
  }

  #[tokio::test]
  async fn an_inbound_half_that_only_ever_errors_is_a_dead_link() {
    let (c_out, _fe_in) = mpsc::unbounded_channel();
    let conn = Connection::spawn(FailingConnector {
      out: c_out,
      left: usize::MAX,
      recovers: false,
    });

    assert!(
      link_dies_within(&conn, 200).await,
      "a transport that answers every poll with an error must stop the driver instead of spinning it forever"
    );
  }

  #[tokio::test]
  async fn errors_a_real_frame_interrupts_never_add_up_to_a_dead_link() {
    let (c_out, _fe_in) = mpsc::unbounded_channel();
    let conn = Connection::spawn(FailingConnector {
      out: c_out,
      left: INBOUND_ERROR_LIMIT * 4,
      recovers: true,
    });
    let mut events = conn.events();

    tokio::time::timeout(Duration::from_secs(1), events.recv())
      .await
      .expect("not timed out")
      .expect("a frame decoded between the errors");

    assert!(
      !link_dies_within(&conn, 20).await,
      "errors that a good frame keeps interrupting are noise on a live link, not death"
    );
  }

  #[tokio::test]
  async fn a_transport_that_hangs_up_closes_the_connection() {
    let (c_out, _fe_in) = mpsc::unbounded_channel();
    let (fe_out, c_in) = mpsc::unbounded_channel();
    let conn = Connection::spawn(ChanConnector { out: c_out, inn: c_in });
    drop(fe_out);

    tokio::time::timeout(Duration::from_secs(2), conn.closed())
      .await
      .expect("a hung-up transport is a closed connection, whoever still holds a handle to it");
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
