use std::{
  collections::{HashMap, VecDeque},
  time::Duration,
};

use libbridgething::{
  Priority,
  protocol::PrioritizedFrame,
  wire::{MsgMeta, ResponseMeta, WireCommand, WireEvent, WireRequest},
};
use tokio::sync::{broadcast, mpsc, oneshot};
use uuid::Uuid;

use crate::{
  error::{RequestFailure, SdkError, TransportError},
  handle::MsgHandle,
  protocol::Protocol,
  transport::{Connector, InboundHalf, OutboundHalf},
};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const EVENTS_CAP: usize = 256;
const CMD_CAP: usize = 64;
const QUEUED_SENDS_CAP: usize = 16;

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
  timeout: Duration,
}

impl<P: Protocol> Clone for Connection<P> {
  fn clone(&self) -> Self {
    Self {
      cmd_tx: self.cmd_tx.clone(),
      events_tx: self.events_tx.clone(),
      timeout: self.timeout,
    }
  }
}

impl<P: Protocol> Connection<P> {
  /// split the connector and spawn the driver on the current runtime.
  pub fn spawn<C: Connector<P>>(connector: C) -> Self {
    let (out, inbound) = connector.split();
    let (cmd_tx, cmd_rx) = mpsc::channel(CMD_CAP);
    let (events_tx, _) = broadcast::channel(EVENTS_CAP);
    let driver = Driver {
      out,
      inbound,
      cmd_rx,
      events_tx: events_tx.clone(),
      pending: HashMap::new(),
    };
    tokio::spawn(driver.run());
    Self {
      cmd_tx,
      events_tx,
      timeout: DEFAULT_TIMEOUT,
    }
  }

  /// override the per-request response timeout (default 30s).
  pub fn with_timeout(mut self, timeout: Duration) -> Self {
    self.timeout = timeout;
    self
  }

  /// subscribe to every inbound message that isn't a correlated response (events, broadcasts, stray responses).
  pub fn events(&self) -> broadcast::Receiver<P::InMsg> {
    self.events_tx.subscribe()
  }

  /// fire-and-forget event (`meta = event`).
  pub async fn event<E>(&self, event: E) -> Result<(), SdkError>
  where
    E: WireEvent<P::OutData>,
  {
    self.send_data(MsgMeta::Event, event.into(), Priority::Normal).await
  }

  /// fire-and-forget command (`meta = command`).
  pub async fn command<C>(&self, command: C) -> Result<(), SdkError>
  where
    C: WireCommand<P::OutData>,
  {
    self.send_data(MsgMeta::Command, command.into(), Priority::Normal).await
  }

  ///  send arbitrary out-data with a chosen meta + priority.
  pub async fn send_data(&self, meta: MsgMeta, data: P::OutData, priority: Priority) -> Result<(), SdkError> {
    let msg = P::envelope(Uuid::now_v7(), meta, data);
    self.send_frame(PrioritizedFrame { priority, msg }).await
  }

  /// typed request: ships `meta = request`, awaits the correlated response
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
    match tokio::time::timeout(self.timeout, reply_rx).await {
      Ok(Ok(in_msg)) => R::extract(P::in_data(in_msg)).map_err(RequestFailure::from),
      Ok(Err(_)) => Err(RequestFailure::Disconnected),
      Err(_) => {
        let _ = self.cmd_tx.send(Command::Cancel(id)).await;
        Err(RequestFailure::Timeout)
      }
    }
  }

  /// Send a raw correlated response for an inbound request id.
  pub async fn respond(&self, request_id: Uuid, data: P::OutData) -> Result<(), SdkError> {
    self
      .send_data(MsgMeta::Response(ResponseMeta { request_id }), data, Priority::Normal)
      .await
  }

  /// Typed response: encode `R::Response` and ship it correlated to the request.
  pub async fn respond_to<R>(&self, request_id: Uuid, response: R::Response) -> Result<(), SdkError>
  where
    R: WireRequest<Outbound = P::InData, Inbound = P::OutData>,
  {
    self.respond(request_id, R::encode_response(response)).await
  }

  /// Typed domain-error response for an inbound request.
  pub async fn respond_err<R>(&self, request_id: Uuid, err: R::DomainError) -> Result<(), SdkError>
  where
    R: WireRequest<Outbound = P::InData, Inbound = P::OutData>,
  {
    self.respond(request_id, R::encode_domain_error(err)).await
  }

  /// Build a [`MsgHandle`] bound to an inbound message's id
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

struct Driver<P: Protocol, O: OutboundHalf<P>, I: InboundHalf<P>> {
  out: O,
  inbound: I,
  cmd_rx: mpsc::Receiver<Command<P>>,
  events_tx: broadcast::Sender<P::InMsg>,
  pending: HashMap<Uuid, oneshot::Sender<P::InMsg>>,
}

/// What to do once a queued frame's send completes: ack the producer, or
/// (for requests) drop the pending entry on failure so the awaiter
/// resolves to Disconnected.
enum Completion {
  Ack(oneshot::Sender<Result<(), TransportError>>),
  Request(Uuid),
}

struct Outgoing<P: Protocol> {
  frame: PrioritizedFrame<P::OutMsg>,
  completion: Completion,
}

const fn lane_index(priority: Priority) -> usize {
  match priority {
    Priority::Normal => 0,
    Priority::Bulk => 1,
    Priority::Background => 2,
  }
}

impl<P: Protocol, O: OutboundHalf<P>, I: InboundHalf<P>> Driver<P, O, I> {
  // outbound frames queue per priority lane and drain strictly in lane
  // order, one frame per turn, so a normal frame enqueued mid-transfer
  // goes out ahead of every queued bulk/background fragment.
  async fn run(self) {
    let Driver {
      mut out,
      mut inbound,
      mut cmd_rx,
      events_tx,
      mut pending,
    } = self;

    let mut lanes: [VecDeque<Outgoing<P>>; 3] = [VecDeque::new(), VecDeque::new(), VecDeque::new()];

    loop {
      let queued: usize = lanes.iter().map(|q| q.len()).sum();
      tokio::select! {
        biased;
        cmd = cmd_rx.recv(), if queued < QUEUED_SENDS_CAP => match cmd {
          Some(Command::Send { frame, ack }) => {
            lanes[lane_index(frame.priority)].push_back(Outgoing {
              frame,
              completion: Completion::Ack(ack),
            });
          }
          Some(Command::Request { id, frame, reply }) => {
            pending.insert(id, reply);
            lanes[lane_index(frame.priority)].push_back(Outgoing {
              frame,
              completion: Completion::Request(id),
            });
          }
          Some(Command::Cancel(id)) => {
            pending.remove(&id);
          }
          None => break,
        },
        inbound_msg = inbound.recv() => match inbound_msg {
          Some(Ok(msg)) => route_inbound::<P>(&events_tx, &mut pending, msg),
          Some(Err(err)) => tracing::warn!(error = %err, "inbound message error"),
          None => break,
        },
        _ = std::future::ready(()), if queued > 0 => {
          let next = lanes.iter_mut().find_map(VecDeque::pop_front).expect("queued > 0");
          let result = out.send(next.frame).await;
          match next.completion {
            Completion::Ack(ack) => {
              let _ = ack.send(result);
            }
            Completion::Request(id) => {
              if let Err(err) = result {
                pending.remove(&id);
                tracing::warn!(%id, error = %err, "request send failed");
              }
            }
          }
        }
      }
    }

    pending.clear();
  }
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
