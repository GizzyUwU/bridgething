use std::{
  sync::{Arc, Mutex},
  time::Duration,
};

use bridgething_gateway::{Gateway, connect_seam_ws};
use bridgething_io::{WsConnect, WsFrame, WsInbox, WsTransport};
use bridgething_sdk_runtime::Connector;
use libbridgething::{
  GatewayCapabilities, GatewayInfo,
  gateway::{BridgeToGatewayMsg, BridgeToGatewayMsgData, GatewayToBridgeCapabilitiesMsgEvent, GatewayToBridgeMsgData},
  protocol::{BridgeEndec, PrioritizedFrame},
  wire::{MsgMeta, WireError},
};
use tokio_util::{
  bytes::BytesMut,
  codec::{Decoder, Encoder},
};
use uuid::Uuid;

#[derive(Default)]
struct FakeWs {
  refuse: Option<String>,
  state: Mutex<State>,
}

#[derive(Default)]
struct State {
  inbox: Option<Arc<WsInbox>>,
  id: Option<Uuid>,
  sent: Vec<Vec<u8>>,
  disconnected: Vec<Uuid>,
}

impl WsTransport for FakeWs {
  fn connect(&self, connect: WsConnect, inbox: Arc<WsInbox>) {
    if let Some(reason) = &self.refuse {
      inbox.on_closed(connect.id, None, reason.clone());
      return;
    }
    let mut state = self.state.lock().unwrap();
    state.id = Some(connect.id);
    inbox.on_open(connect.id, None);
    state.inbox = Some(inbox);
  }

  fn send(&self, _id: Uuid, frame: WsFrame) {
    if let WsFrame::Binary(bytes) = frame {
      self.state.lock().unwrap().sent.push(bytes);
    }
  }

  fn disconnect(&self, id: Uuid, _code: Option<u16>, _reason: Option<String>) {
    self.state.lock().unwrap().disconnected.push(id);
  }
}

impl FakeWs {
  fn refusing(reason: &str) -> Arc<Self> {
    Arc::new(FakeWs {
      refuse: Some(reason.to_string()),
      state: Mutex::default(),
    })
  }

  fn deliver(&self, bytes: Vec<u8>) {
    let state = self.state.lock().unwrap();
    let (inbox, id) = (state.inbox.as_ref().unwrap(), state.id.unwrap());
    inbox.on_binary(id, bytes);
  }

  async fn next_sent(&self) -> Vec<u8> {
    for _ in 0..200 {
      if let Some(bytes) = self.state.lock().unwrap().sent.first().cloned() {
        return bytes;
      }
      tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the core never wrote a frame to the transport");
  }
}

fn announce() -> GatewayToBridgeCapabilitiesMsgEvent {
  GatewayToBridgeCapabilitiesMsgEvent::Announce(GatewayCapabilities {
    gateway: GatewayInfo {
      address: String::new(),
      name: "seam-test".into(),
      os_name: "linux".into(),
      app_name: "seam-test".into(),
      app_version: "0.0.0".into(),
      adapter_version: "test".into(),
      lib_version: "0.0.0".into(),
      libbridgething_version: format!("v{}", libbridgething::LIBBRIDGETHING_VERSION),
    },
    ..Default::default()
  })
}

fn encode_inbound(msg: BridgeToGatewayMsg) -> Vec<u8> {
  let mut dst = BytesMut::new();
  BridgeEndec::default()
    .encode(PrioritizedFrame::normal(msg), &mut dst)
    .expect("encode");
  dst.to_vec()
}

#[tokio::test]
async fn a_refused_dial_reports_rather_than_waiting_forever() {
  let transport = FakeWs::refusing("no route to host");

  let Err(failure) = connect_seam_ws(transport, "wss://device.invalid/gateway").await else {
    panic!("a transport that closes the socket cannot yield a link");
  };

  assert!(
    failure.to_string().contains("no route to host"),
    "the transport's own reason survives: {failure}"
  );
}

#[derive(Default)]
struct MuteWs {
  held: Mutex<Vec<Arc<WsInbox>>>,
}

impl WsTransport for MuteWs {
  fn connect(&self, _connect: WsConnect, inbox: Arc<WsInbox>) {
    self.held.lock().unwrap().push(inbox);
  }
  fn send(&self, _id: Uuid, _frame: WsFrame) {}
  fn disconnect(&self, _id: Uuid, _code: Option<u16>, _reason: Option<String>) {}
}

#[tokio::test(start_paused = true)]
async fn a_dial_the_transport_never_answers_gives_up_rather_than_waiting_forever() {
  let Err(failure) = connect_seam_ws(Arc::new(MuteWs::default()), "wss://device.invalid/gateway").await else {
    panic!("a transport that never reports the socket up cannot yield a link");
  };

  assert!(
    failure.to_string().contains("timed out"),
    "a stuck handshake has to surface as a timeout: {failure}"
  );
}

#[tokio::test]
async fn an_outbound_event_reaches_the_transport_as_one_binary_frame() {
  let transport = Arc::new(FakeWs::default());
  let connector = connect_seam_ws(transport.clone(), "wss://device.invalid/gateway")
    .await
    .expect("the fake reports the socket up");

  let gateway = Gateway::spawn(connector);
  gateway.event(announce()).await.expect("send announce");

  let mut bytes = BytesMut::from(&transport.next_sent().await[..]);
  let frame = BridgeEndec::default()
    .decode(&mut bytes)
    .expect("decodes")
    .expect("a whole frame")
    .frame()
    .expect("a good frame");
  assert!(matches!(
    frame.msg.data,
    GatewayToBridgeMsgData::Capabilities(libbridgething::gateway::GatewayToBridgeCapabilitiesMsg::Announce(_))
  ));
}

#[tokio::test]
async fn bytes_the_transport_reports_surface_on_the_events_stream() {
  let transport = Arc::new(FakeWs::default());
  let connector = connect_seam_ws(transport.clone(), "wss://device.invalid/gateway")
    .await
    .expect("the fake reports the socket up");

  let (gateway, mut events) = Gateway::spawn_subscribed(connector);
  transport.deliver(encode_inbound(BridgeToGatewayMsg {
    id: Uuid::now_v7(),
    meta: MsgMeta::Event,
    data: BridgeToGatewayMsgData::Error(WireError::Unsupported),
  }));

  let got = tokio::time::timeout(Duration::from_secs(2), events.recv())
    .await
    .expect("not timed out")
    .expect("an event");
  assert!(matches!(
    got.data,
    BridgeToGatewayMsgData::Error(WireError::Unsupported)
  ));
  drop(gateway);
}

#[tokio::test]
async fn dropping_the_link_tells_the_transport_to_let_go() {
  let transport = Arc::new(FakeWs::default());
  let connector = connect_seam_ws(transport.clone(), "wss://device.invalid/gateway")
    .await
    .expect("the fake reports the socket up");
  let id = transport.state.lock().unwrap().id.unwrap();

  let (out, inbound) = connector.split();
  drop(out);
  assert!(
    transport.state.lock().unwrap().disconnected.is_empty(),
    "one surviving half still owns the socket"
  );

  drop(inbound);
  assert_eq!(
    transport.state.lock().unwrap().disconnected.as_slice(),
    [id],
    "the last half releases it exactly once"
  );
}
