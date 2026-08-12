use std::{
  collections::VecDeque,
  sync::{Arc, Mutex},
  time::Duration,
};

use bridgething_gateway::Gateway;
use futures::{SinkExt, StreamExt};
use libbridgething::{
  Priority,
  gateway::{BridgeToGatewayMsg, BridgeToGatewayMsgData, GatewayToBridgeMsg, GatewayToBridgeMsgData},
  protocol::{BridgeEndec, DecodedFrame},
  wire::{MsgMeta, ResponseMeta},
};
use tokio::sync::mpsc;
use tokio_util::codec::Framed;
use uuid::Uuid;

pub const WIRE_TIMEOUT: Duration = Duration::from_secs(3);

pub fn pattern(len: usize) -> Vec<u8> {
  (0..len).map(|i| (i % 251) as u8).collect()
}

pub struct FakeDevice {
  outbound: mpsc::UnboundedReceiver<GatewayToBridgeMsg>,
  inbound: mpsc::UnboundedSender<BridgeToGatewayMsg>,
  pending: VecDeque<GatewayToBridgeMsg>,
  lanes: Arc<Mutex<Vec<(Priority, GatewayToBridgeMsgData)>>>,
}

pub fn linked_gateway() -> (Gateway, FakeDevice) {
  let (companion_io, device_io) = tokio::io::duplex(1024 * 1024);
  let (out_tx, out_rx) = mpsc::unbounded_channel();
  let (in_tx, mut in_rx) = mpsc::unbounded_channel::<BridgeToGatewayMsg>();
  let lanes = Arc::new(Mutex::new(Vec::new()));
  let recorded = lanes.clone();

  tokio::spawn(async move {
    let mut framed = Framed::new(device_io, BridgeEndec::default());
    loop {
      tokio::select! {
        item = framed.next() => match item {
          Some(Ok(DecodedFrame::Frame(frame))) => {
            recorded.lock().unwrap().push((frame.priority, frame.msg.data.clone()));
            let _ = out_tx.send(frame.msg);
          }
          _ => break,
        },
        msg = in_rx.recv() => match msg {
          Some(msg) => { let _ = framed.send(msg).await; }
          None => break,
        },
      }
    }
  });

  (
    Gateway::from_io(companion_io),
    FakeDevice {
      outbound: out_rx,
      inbound: in_tx,
      pending: VecDeque::new(),
      lanes,
    },
  )
}

impl FakeDevice {
  pub fn lanes_of<T>(&self, pick: impl Fn(&GatewayToBridgeMsgData) -> Option<T>) -> Vec<Priority> {
    self
      .lanes
      .lock()
      .unwrap()
      .iter()
      .filter(|(_, data)| pick(data).is_some())
      .map(|(priority, _)| *priority)
      .collect()
  }

  pub async fn next_matching<T>(&mut self, pick: impl Fn(&GatewayToBridgeMsg) -> Option<T>) -> T {
    self.next_matching_within(WIRE_TIMEOUT, pick).await
  }

  pub async fn next_matching_within<T>(
    &mut self,
    window: Duration,
    pick: impl Fn(&GatewayToBridgeMsg) -> Option<T>,
  ) -> T {
    if let Some(at) = self.pending.iter().position(|msg| pick(msg).is_some()) {
      let msg = self.pending.remove(at).expect("the position just found");
      return pick(&msg).expect("the predicate that just matched");
    }
    let deadline = tokio::time::Instant::now() + window;
    loop {
      let msg = tokio::time::timeout_at(deadline, self.outbound.recv())
        .await
        .expect("the subject went quiet while a message was expected")
        .expect("the link closed");
      if let Some(hit) = pick(&msg) {
        return hit;
      }
      self.pending.push_back(msg);
    }
  }

  pub async fn nothing_matching<T>(
    &mut self,
    window: Duration,
    pick: impl Fn(&GatewayToBridgeMsg) -> Option<T>,
  ) -> bool {
    if self.pending.iter().any(|msg| pick(msg).is_some()) {
      return false;
    }
    let deadline = tokio::time::Instant::now() + window;
    loop {
      match tokio::time::timeout_at(deadline, self.outbound.recv()).await {
        Err(_) => return true,
        Ok(None) => return true,
        Ok(Some(msg)) => {
          if pick(&msg).is_some() {
            return false;
          }
          self.pending.push_back(msg);
        }
      }
    }
  }

  pub fn event(&self, data: BridgeToGatewayMsgData) {
    let _ = self.inbound.send(BridgeToGatewayMsg {
      id: Uuid::now_v7(),
      meta: MsgMeta::Event,
      data,
    });
  }

  pub fn request(&self, data: BridgeToGatewayMsgData) -> Uuid {
    let id = Uuid::now_v7();
    let _ = self.inbound.send(BridgeToGatewayMsg {
      id,
      meta: MsgMeta::Request,
      data,
    });
    id
  }

  pub fn respond(&self, request_id: Uuid, data: BridgeToGatewayMsgData) {
    let _ = self.inbound.send(BridgeToGatewayMsg {
      id: Uuid::now_v7(),
      meta: MsgMeta::Response(ResponseMeta { request_id }),
      data,
    });
  }
}
