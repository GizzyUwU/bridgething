use std::{collections::VecDeque, sync::Arc, time::Duration};

use bytes::{Bytes, BytesMut};
use libbridgething::{Priority, protocol::PrioritizedFrame, wire::MsgMeta};
use tokio::sync::{Mutex as AsyncMutex, mpsc};
use uuid::Uuid;

use crate::{
  Connection, Connector, InboundHalf, LaneLimits, LaneScheduler, OutboundHalf, Protocol, SdkError, TransportError,
  lane_corpus::{
    CaseIn, Emission, Emitted, EmittedCase, EmittedStep, Op, Segment, assert_conforms, constants, corpus, write_trace,
  },
};

const fn lane_index(priority: Priority) -> usize {
  match priority {
    Priority::Normal => 0,
    Priority::Bulk => 1,
    Priority::Background => 2,
  }
}

fn build_frame(priority: Priority, byte_len: usize) -> Bytes {
  let mut header = [0u8; 16];
  header[0] = 0xDE;
  header[1] = 0xAD;
  header[2] = 0x02;
  header[5] = priority.as_byte();
  header[8..16].copy_from_slice(&(byte_len as u64).to_be_bytes());

  let head_len = byte_len.min(16);
  let mut buf = BytesMut::with_capacity(byte_len);
  buf.extend_from_slice(&header[..head_len]);
  if byte_len > 16 {
    buf.resize(byte_len, 0xA5);
  }
  buf.freeze()
}

#[derive(Default)]
struct Attribution {
  lanes: [VecDeque<(String, usize)>; 3],
}

impl Attribution {
  fn queued(&mut self, id: &str, priority: Priority, byte_len: usize) {
    self.lanes[lane_index(priority)].push_back((id.to_string(), byte_len));
  }

  fn holding(&self, lane: usize) -> usize {
    self.lanes[lane].len()
  }

  fn attribute(&mut self, batch: &[u8]) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut cursor = 0usize;
    while cursor < batch.len() {
      let lane = lane_index(Priority::from_byte(batch[cursor + 5]));
      let (id, bytes) = self.lanes[lane]
        .pop_front()
        .expect("emitted segment has no matching pending frame in its lane");
      segments.push(Segment {
        id,
        bytes: bytes as u64,
      });
      cursor += bytes;
    }
    segments
  }
}

struct PackerArm {
  sched: LaneScheduler<Bytes>,
  attribution: Attribution,
  lane_depth: usize,
  waiting: [VecDeque<(String, Priority, usize)>; 3],
  parked: Vec<String>,
}

impl PackerArm {
  fn new(case: &CaseIn) -> Self {
    Self {
      sched: LaneScheduler::new(case.max_emission_bytes()),
      attribution: Attribution::default(),
      lane_depth: case.lane_depth(),
      waiting: Default::default(),
      parked: Vec::new(),
    }
  }

  fn admit(&mut self) {
    for lane in 0..3 {
      while self.attribution.holding(lane) < self.lane_depth {
        let Some((id, priority, byte_len)) = self.waiting[lane].pop_front() else {
          break;
        };
        self.attribution.queued(&id, priority, byte_len);
        self.sched.push(priority, build_frame(priority, byte_len));
        self.parked.retain(|waiting| waiting != &id);
      }
    }
  }

  fn enqueue(&mut self, id: String, priority: Priority, byte_len: usize) -> EmittedStep {
    self.waiting[lane_index(priority)].push_back((id.clone(), priority, byte_len));
    self.parked.push(id.clone());
    self.admit();

    let mut step = EmittedStep::new("enqueue");
    step.outcome = Some(
      if self.parked.contains(&id) {
        "parked"
      } else {
        "accepted"
      }
      .to_string(),
    );
    step
  }

  fn drain(&mut self) -> EmittedStep {
    let mut step = EmittedStep::new("drain");
    if let Some(batch) = self.sched.next_batch() {
      step.segments = self.attribution.attribute(&batch.into_bytes());
      self.admit();
    }
    step
  }

  fn finish(&self, step: &mut EmittedStep) {
    step.parked_ids = self.parked.clone();
    step.queued_bytes = Some(self.sched.queued_bytes() as u64);
  }
}

fn run_packer_case(case: &CaseIn) -> EmittedCase {
  let mut arm = PackerArm::new(case);
  let mut steps = Vec::new();
  for op in case.expand() {
    let mut step = match op {
      Op::Enqueue {
        id, priority, byte_len, ..
      } => arm.enqueue(id, priority, byte_len),
      Op::Drain => arm.drain(),
      Op::WriteComplete => EmittedStep::new("write_complete"),
    };
    arm.finish(&mut step);
    steps.push(step);
  }
  EmittedCase {
    name: case.name.clone(),
    steps,
  }
}

#[test]
fn rust_packer_conforms_to_the_frozen_expectation() {
  let emitted = Emitted {
    implementation: "rust-packer",
    constants: constants(),
    cases: corpus().cases.iter().map(run_packer_case).collect(),
  };
  write_trace(&emitted);
  assert_conforms(&emitted, Emission::Exact);
}

#[derive(Clone)]
struct TraceMsg {
  priority: Priority,
  byte_len: usize,
}

struct TraceProto;

static EVENT_META: MsgMeta = MsgMeta::Event;

impl Protocol for TraceProto {
  type OutData = TraceMsg;
  type InData = TraceMsg;
  type OutMsg = TraceMsg;
  type InMsg = TraceMsg;

  fn envelope(_id: Uuid, _meta: MsgMeta, data: TraceMsg) -> TraceMsg {
    data
  }
  fn in_id(_msg: &TraceMsg) -> Uuid {
    Uuid::nil()
  }
  fn in_meta(_msg: &TraceMsg) -> &MsgMeta {
    &EVENT_META
  }
  fn in_data(msg: TraceMsg) -> TraceMsg {
    msg
  }
}

struct Link {
  open: bool,
  emissions: Vec<Bytes>,
}

type Shared = Arc<AsyncMutex<Link>>;

struct GatedConnector {
  shared: Shared,
  max_emission_bytes: usize,
  inn: mpsc::UnboundedReceiver<TraceMsg>,
}
struct GatedOut {
  shared: Shared,
  max_emission_bytes: usize,
}
struct GatedIn {
  inn: mpsc::UnboundedReceiver<TraceMsg>,
}

const POLL: Duration = Duration::from_millis(1);
const SETTLE: Duration = Duration::from_millis(60);

impl Connector<TraceProto> for GatedConnector {
  type Out = GatedOut;
  type In = GatedIn;
  fn split(self) -> (GatedOut, GatedIn) {
    (
      GatedOut {
        shared: self.shared,
        max_emission_bytes: self.max_emission_bytes,
      },
      GatedIn { inn: self.inn },
    )
  }
}

impl OutboundHalf<TraceProto> for GatedOut {
  fn max_batch_bytes(&self) -> usize {
    self.max_emission_bytes
  }

  fn encode(frame: PrioritizedFrame<TraceMsg>) -> Result<Bytes, TransportError> {
    Ok(build_frame(frame.msg.priority, frame.msg.byte_len))
  }

  async fn ready(&mut self) -> Result<(), TransportError> {
    loop {
      {
        let mut link = self.shared.lock().await;
        if link.open {
          link.open = false;
          return Ok(());
        }
      }
      tokio::time::sleep(POLL).await;
    }
  }

  async fn send_batch(&mut self, batch: Bytes) -> Result<(), TransportError> {
    self.shared.lock().await.emissions.push(batch);
    Ok(())
  }
}

impl InboundHalf<TraceProto> for GatedIn {
  async fn recv(&mut self) -> Option<Result<TraceMsg, TransportError>> {
    self.inn.recv().await.map(Ok)
  }
}

type SendTask = tokio::task::JoinHandle<Result<(), SdkError>>;

struct Parked {
  id: String,
  byte_len: usize,
  task: SendTask,
}

struct DriverArm {
  conn: Connection<TraceProto>,
  shared: Shared,
  attribution: Attribution,
  parked: Vec<Parked>,
  _keepalive: mpsc::UnboundedSender<TraceMsg>,
}

impl DriverArm {
  fn new(case: &CaseIn) -> Self {
    let shared: Shared = Arc::new(AsyncMutex::new(Link {
      open: false,
      emissions: Vec::new(),
    }));
    let (keepalive, inn) = mpsc::unbounded_channel();
    let conn = Connection::spawn_with(
      GatedConnector {
        shared: shared.clone(),
        max_emission_bytes: case.max_emission_bytes(),
        inn,
      },
      LaneLimits {
        max_lane_bytes: case.max_lane_bytes(),
      },
    );
    Self {
      conn,
      shared,
      attribution: Attribution::default(),
      parked: Vec::new(),
      _keepalive: keepalive,
    }
  }

  async fn enqueue(&mut self, id: String, priority: Priority, byte_len: usize) -> EmittedStep {
    self.attribution.queued(&id, priority, byte_len);
    let conn = self.conn.clone();
    let msg = TraceMsg { priority, byte_len };
    let mut task = tokio::spawn(async move { conn.send_data(MsgMeta::Event, msg, priority).await });

    let accepted = tokio::select! {
      res = &mut task => {
        res.expect("enqueue task panicked").expect("send_data succeeds against a live driver");
        true
      }
      _ = tokio::time::sleep(SETTLE) => false,
    };
    if !accepted {
      self.parked.push(Parked { id, byte_len, task });
    }

    let mut step = EmittedStep::new("enqueue");
    step.outcome = Some(if accepted { "accepted" } else { "parked" }.to_string());
    step
  }

  async fn drain(&mut self) -> EmittedStep {
    let before = {
      let mut link = self.shared.lock().await;
      link.open = true;
      link.emissions.len()
    };

    let deadline = tokio::time::Instant::now() + SETTLE;
    let emitted = loop {
      {
        let mut link = self.shared.lock().await;
        if link.emissions.len() > before {
          break Some(link.emissions[before].clone());
        }
        if tokio::time::Instant::now() >= deadline {
          link.open = false;
          break None;
        }
      }
      tokio::time::sleep(POLL).await;
    };

    let mut step = EmittedStep::new("drain");
    if let Some(bytes) = emitted {
      step.segments = self.attribution.attribute(&bytes);
      tokio::time::sleep(SETTLE).await;
    }
    step
  }

  async fn finish(&mut self, step: &mut EmittedStep) {
    let mut still_parked = Vec::new();
    for item in std::mem::take(&mut self.parked) {
      if item.task.is_finished() {
        item
          .task
          .await
          .expect("parked task panicked")
          .expect("send_data succeeds against a live driver");
      } else {
        still_parked.push(item);
      }
    }
    let handed: u64 = self
      .attribution
      .lanes
      .iter()
      .flatten()
      .map(|(_, len)| *len as u64)
      .sum();
    let waiting: u64 = still_parked.iter().map(|item| item.byte_len as u64).sum();
    step.parked_ids = still_parked.iter().map(|item| item.id.clone()).collect();
    step.queued_bytes = Some(handed - waiting);
    self.parked = still_parked;
  }
}

async fn run_driver_case(case: &CaseIn) -> EmittedCase {
  let mut arm = DriverArm::new(case);
  let mut steps = Vec::new();
  for op in case.expand() {
    let mut step = match op {
      Op::Enqueue {
        id, priority, byte_len, ..
      } => arm.enqueue(id, priority, byte_len).await,
      Op::Drain => arm.drain().await,
      Op::WriteComplete => EmittedStep::new("write_complete"),
    };
    arm.finish(&mut step).await;
    steps.push(step);
  }

  for item in std::mem::take(&mut arm.parked) {
    item.task.abort();
  }

  EmittedCase {
    name: case.name.clone(),
    steps,
  }
}

#[tokio::test(start_paused = true)]
async fn rust_driver_conforms_to_the_frozen_expectation() {
  let corpus = corpus();
  let mut cases = Vec::new();
  for case in &corpus.cases {
    cases.push(run_driver_case(case).await);
  }
  let emitted = Emitted {
    implementation: "rust-driver",
    constants: constants(),
    cases,
  };
  write_trace(&emitted);
  assert_conforms(&emitted, Emission::Exact);
}
