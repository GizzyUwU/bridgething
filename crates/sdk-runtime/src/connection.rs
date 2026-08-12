use std::{
  collections::{HashMap, VecDeque},
  future::Future,
  mem,
  pin::Pin,
  time::Duration,
};

use bytes::Bytes;
use libbridgething::{
  Priority,
  protocol::{Compress, PrioritizedFrame},
  wire::{MsgMeta, ResponseMeta, WireCommand, WireEvent, WireRequest},
};
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use uuid::Uuid;

use crate::{
  error::{RequestFailure, SdkError, TransportError},
  handle::MsgHandle,
  protocol::Protocol,
  rt,
  sched::LaneScheduler,
  transport::{Connector, InboundHalf, OutboundHalf},
};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const EVENTS_CAP: usize = 256;
const CMD_CAP: usize = 64;
const DEFAULT_MAX_LANE_BYTES: usize = 256 * 1024;
pub(crate) const INBOUND_ERROR_LIMIT: usize = 8;

const LANES: [Priority; 3] = [Priority::Normal, Priority::Bulk, Priority::Background];

const fn lane_index(priority: Priority) -> usize {
  match priority {
    Priority::Normal => 0,
    Priority::Bulk => 1,
    Priority::Background => 2,
  }
}

#[derive(Debug, Clone, Copy)]
pub struct LaneLimits {
  pub max_lane_bytes: usize,
}

impl Default for LaneLimits {
  fn default() -> Self {
    Self {
      max_lane_bytes: DEFAULT_MAX_LANE_BYTES,
    }
  }
}

enum Command<P: Protocol> {
  Send {
    frame: PrioritizedFrame<P::OutMsg>,
    ack: oneshot::Sender<Result<(), TransportError>>,
  },
  Request {
    id: Uuid,
    frame: PrioritizedFrame<P::OutMsg>,
    reply: oneshot::Sender<P::InMsg>,
  },
  Cancel(Uuid),
}

pub struct Connection<P: Protocol> {
  cmd_tx: mpsc::Sender<Command<P>>,
  events_tx: broadcast::Sender<P::InMsg>,
  closed: watch::Receiver<bool>,
  timeout: Duration,
}

impl<P: Protocol> Clone for Connection<P> {
  fn clone(&self) -> Self {
    Self {
      cmd_tx: self.cmd_tx.clone(),
      events_tx: self.events_tx.clone(),
      closed: self.closed.clone(),
      timeout: self.timeout,
    }
  }
}

impl<P: Protocol> Connection<P> {
  pub fn spawn<C: Connector<P>>(connector: C) -> Self {
    Self::spawn_with(connector, LaneLimits::default())
  }

  pub fn spawn_with<C: Connector<P>>(connector: C, limits: LaneLimits) -> Self {
    Self::spawn_subscribed(connector, limits).0
  }

  pub fn spawn_subscribed<C: Connector<P>>(connector: C, limits: LaneLimits) -> (Self, broadcast::Receiver<P::InMsg>) {
    let (out, inbound) = connector.split();
    let max_batch_bytes = out.max_batch_bytes();
    let (cmd_tx, cmd_rx) = mpsc::channel(CMD_CAP);
    let (events_tx, events_rx) = broadcast::channel(EVENTS_CAP);
    let (closed_tx, closed) = watch::channel(false);
    let driver = Driver {
      out,
      inbound,
      cmd_rx,
      events_tx: events_tx.clone(),
      closed: closed_tx,
      pending: HashMap::new(),
      sched: LaneScheduler::new(max_batch_bytes),
      stalled: [VecDeque::new(), VecDeque::new(), VecDeque::new()],
      max_lane_bytes: limits.max_lane_bytes,
    };
    rt::spawn(driver.run());
    (
      Self {
        cmd_tx,
        events_tx,
        closed,
        timeout: DEFAULT_TIMEOUT,
      },
      events_rx,
    )
  }

  pub fn with_timeout(mut self, timeout: Duration) -> Self {
    self.timeout = timeout;
    self
  }

  pub fn events(&self) -> broadcast::Receiver<P::InMsg> {
    self.events_tx.subscribe()
  }

  pub fn closed(&self) -> impl Future<Output = ()> + Send + 'static {
    let mut watching = self.closed.clone();
    async move {
      while !*watching.borrow() {
        if watching.changed().await.is_err() {
          return;
        }
      }
    }
  }

  pub async fn event<E>(&self, event: E) -> Result<(), SdkError>
  where
    E: WireEvent<P::OutData>,
  {
    self.send_data(MsgMeta::Event, event.into(), Priority::Normal).await
  }

  pub async fn command<C>(&self, command: C) -> Result<(), SdkError>
  where
    C: WireCommand<P::OutData>,
  {
    self.send_data(MsgMeta::Command, command.into(), Priority::Normal).await
  }

  pub async fn send_data(&self, meta: MsgMeta, data: P::OutData, priority: Priority) -> Result<(), SdkError> {
    self.send_framed(meta, data, priority, Compress::Auto).await
  }

  pub async fn send_framed(
    &self,
    meta: MsgMeta,
    data: P::OutData,
    priority: Priority,
    compress: Compress,
  ) -> Result<(), SdkError> {
    let msg = P::envelope(Uuid::now_v7(), meta, data);
    self
      .send_frame(PrioritizedFrame::new(priority, msg).compressed(compress))
      .await
  }

  pub async fn request<R>(&self, request: R) -> Result<R::Response, RequestFailure<R::DomainError>>
  where
    R: WireRequest<Outbound = P::OutData, Inbound = P::InData>,
  {
    let id = Uuid::now_v7();
    let msg = P::envelope(id, MsgMeta::Request, request.into());
    let (reply, reply_rx) = oneshot::channel();
    if self
      .cmd_tx
      .send(Command::Request {
        id,
        frame: PrioritizedFrame::normal(msg),
        reply,
      })
      .await
      .is_err()
    {
      return Err(RequestFailure::Disconnected);
    }
    match rt::timeout(self.timeout, reply_rx).await {
      Ok(Ok(in_msg)) => R::extract(P::in_data(in_msg)).map_err(RequestFailure::from),
      Ok(Err(_)) => Err(RequestFailure::Disconnected),
      Err(_) => {
        let _ = self.cmd_tx.send(Command::Cancel(id)).await;
        Err(RequestFailure::Timeout)
      }
    }
  }

  pub async fn respond(&self, request_id: Uuid, data: P::OutData) -> Result<(), SdkError> {
    self
      .send_data(MsgMeta::Response(ResponseMeta { request_id }), data, Priority::Normal)
      .await
  }

  pub async fn respond_to<R>(&self, request_id: Uuid, response: R::Response) -> Result<(), SdkError>
  where
    R: WireRequest<Outbound = P::InData, Inbound = P::OutData>,
  {
    self
      .respond_to_with::<R>(request_id, response, Priority::Normal, Compress::Auto)
      .await
  }

  pub async fn respond_to_with<R>(
    &self,
    request_id: Uuid,
    response: R::Response,
    priority: Priority,
    compress: Compress,
  ) -> Result<(), SdkError>
  where
    R: WireRequest<Outbound = P::InData, Inbound = P::OutData>,
  {
    self
      .send_framed(
        MsgMeta::Response(ResponseMeta { request_id }),
        R::encode_response(response),
        priority,
        compress,
      )
      .await
  }

  pub async fn respond_err<R>(&self, request_id: Uuid, err: R::DomainError) -> Result<(), SdkError>
  where
    R: WireRequest<Outbound = P::InData, Inbound = P::OutData>,
  {
    self.respond(request_id, R::encode_domain_error(err)).await
  }

  pub fn handle(&self, msg: &P::InMsg) -> MsgHandle<P> {
    MsgHandle::new(self.clone(), P::in_id(msg), P::in_meta(msg).clone())
  }

  async fn send_frame(&self, frame: PrioritizedFrame<P::OutMsg>) -> Result<(), SdkError> {
    let (ack, ack_rx) = oneshot::channel();
    self
      .cmd_tx
      .send(Command::Send { frame, ack })
      .await
      .map_err(|_| SdkError::Disconnected)?;
    ack_rx
      .await
      .map_err(|_| SdkError::Disconnected)?
      .map_err(SdkError::Transport)
  }
}

enum Completion {
  Ack(oneshot::Sender<Result<(), TransportError>>),
  Request(Uuid),
}

struct Stalled<P: Protocol> {
  frame: PrioritizedFrame<P::OutMsg>,
  completion: Completion,
}

type LinkFut<O> = Pin<Box<dyn Future<Output = (O, Result<(), TransportError>)> + Send>>;

enum Link<O> {
  Idle(O),
  Ready(LinkFut<O>),
  Writing(LinkFut<O>),
  Gone,
}

#[derive(Clone, Copy)]
enum Phase {
  Idle,
  Ready,
  Writing,
}

impl<O> Link<O> {
  fn phase(&self) -> Phase {
    match self {
      Link::Ready(_) => Phase::Ready,
      Link::Writing(_) => Phase::Writing,
      Link::Idle(_) | Link::Gone => Phase::Idle,
    }
  }
}

async fn ready_step<P: Protocol, O: OutboundHalf<P>>(mut out: O) -> (O, Result<(), TransportError>) {
  let result = out.ready().await;
  (out, result)
}

async fn write_step<P: Protocol, O: OutboundHalf<P>>(mut out: O, batch: Bytes) -> (O, Result<(), TransportError>) {
  let result = out.send_batch(batch).await;
  (out, result)
}

async fn link_step<O>(link: &mut Link<O>) -> (O, Result<(), TransportError>) {
  match link {
    Link::Ready(fut) | Link::Writing(fut) => fut.as_mut().await,
    Link::Idle(_) | Link::Gone => std::future::pending().await,
  }
}

fn request_capacity<P: Protocol, O: OutboundHalf<P>>(link: &mut Link<O>, sched: &LaneScheduler<Bytes>) -> bool {
  if sched.is_empty() || !matches!(link, Link::Idle(_)) {
    return false;
  }
  let Link::Idle(out) = mem::replace(link, Link::Gone) else {
    unreachable!("checked above")
  };
  *link = Link::Ready(Box::pin(ready_step::<P, O>(out)));
  true
}

struct Driver<P: Protocol, O: OutboundHalf<P>, I: InboundHalf<P>> {
  out: O,
  inbound: I,
  cmd_rx: mpsc::Receiver<Command<P>>,
  events_tx: broadcast::Sender<P::InMsg>,
  closed: watch::Sender<bool>,
  pending: HashMap<Uuid, oneshot::Sender<P::InMsg>>,
  sched: LaneScheduler<Bytes>,
  stalled: [VecDeque<Stalled<P>>; 3],
  max_lane_bytes: usize,
}

enum Wake<P: Protocol, O> {
  Link((O, Result<(), TransportError>)),
  Cmd(Option<Command<P>>),
  Inbound(Option<Result<P::InMsg, TransportError>>),
}

impl<P: Protocol, O: OutboundHalf<P>, I: InboundHalf<P>> Driver<P, O, I> {
  async fn run(self) {
    let Driver {
      out,
      mut inbound,
      mut cmd_rx,
      events_tx,
      closed,
      mut pending,
      mut sched,
      mut stalled,
      max_lane_bytes,
    } = self;

    let mut link = Link::Idle(out);
    let mut inbound_errors = 0usize;

    loop {
      loop {
        let admitted = admit::<P, O>(&mut sched, &mut stalled, &mut pending, max_lane_bytes);
        let asked = request_capacity::<P, O>(&mut link, &sched);
        if !admitted && !asked {
          break;
        }
      }

      let phase = link.phase();
      let wake = tokio::select! {
        biased;
        done = link_step(&mut link) => Wake::Link(done),
        cmd = cmd_rx.recv() => Wake::Cmd(cmd),
        msg = inbound.recv() => Wake::Inbound(msg),
      };

      match wake {
        Wake::Link((out, result)) => {
          if let Err(err) = result {
            tracing::warn!(error = %err, "outbound link failed; closing");
            break;
          }
          link = match phase {
            Phase::Ready => match sched.next_batch() {
              Some(batch) => Link::Writing(Box::pin(write_step::<P, O>(out, batch.into_bytes()))),
              None => Link::Idle(out),
            },
            Phase::Writing | Phase::Idle => Link::Idle(out),
          };
        }
        Wake::Cmd(cmd) => match cmd {
          Some(Command::Send { frame, ack }) => stall(&mut stalled, frame, Completion::Ack(ack)),
          Some(Command::Request { id, frame, reply }) => {
            pending.insert(id, reply);
            stall(&mut stalled, frame, Completion::Request(id));
          }
          Some(Command::Cancel(id)) => {
            pending.remove(&id);
          }
          None => break,
        },
        Wake::Inbound(msg) => match msg {
          Some(Ok(msg)) => {
            inbound_errors = 0;
            route_inbound::<P>(&events_tx, &mut pending, msg);
          }
          Some(Err(TransportError::Closed)) => {
            tracing::warn!("inbound link closed");
            break;
          }
          Some(Err(err)) => {
            inbound_errors += 1;
            tracing::warn!(error = %err, inbound_errors, "inbound message error");
            if inbound_errors >= INBOUND_ERROR_LIMIT {
              tracing::warn!("inbound link failed {INBOUND_ERROR_LIMIT} times without a frame; closing");
              break;
            }
          }
          None => break,
        },
      }
    }

    pending.clear();
    let _ = closed.send(true);
  }
}

fn stall<P: Protocol>(
  stalled: &mut [VecDeque<Stalled<P>>; 3],
  frame: PrioritizedFrame<P::OutMsg>,
  completion: Completion,
) {
  stalled[lane_index(frame.priority)].push_back(Stalled { frame, completion });
}

fn admit<P: Protocol, O: OutboundHalf<P>>(
  sched: &mut LaneScheduler<Bytes>,
  stalled: &mut [VecDeque<Stalled<P>>; 3],
  pending: &mut HashMap<Uuid, oneshot::Sender<P::InMsg>>,
  max_lane_bytes: usize,
) -> bool {
  let mut progressed = false;
  for priority in LANES {
    let lane = lane_index(priority);
    while sched.lane_bytes(priority) < max_lane_bytes {
      let Some(item) = stalled[lane].pop_front() else { break };
      progressed = true;
      match O::encode(item.frame) {
        Ok(bytes) => {
          sched.push(priority, bytes);
          if let Completion::Ack(ack) = item.completion {
            let _ = ack.send(Ok(()));
          }
        }
        Err(err) => match item.completion {
          Completion::Ack(ack) => {
            let _ = ack.send(Err(err));
          }
          Completion::Request(id) => {
            pending.remove(&id);
            tracing::warn!(%id, error = %err, "request encode failed");
          }
        },
      }
    }
  }
  progressed
}

fn route_inbound<P: Protocol>(
  events_tx: &broadcast::Sender<P::InMsg>,
  pending: &mut HashMap<Uuid, oneshot::Sender<P::InMsg>>,
  msg: P::InMsg,
) {
  if let MsgMeta::Response(ResponseMeta { request_id }) = P::in_meta(&msg) {
    let request_id = *request_id;
    if let Some(tx) = pending.remove(&request_id) {
      let _ = tx.send(msg);
      return;
    }
  }
  let _ = events_tx.send(msg);
}
