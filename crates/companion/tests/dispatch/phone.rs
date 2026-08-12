use std::sync::{
  Arc, Mutex,
  atomic::{AtomicUsize, Ordering},
};

use bridgething_companion::{
  backend::{
    AcceptCallAction, CallEndReason, CommunicationsState, DtmfTone, EndCallAction, InitiateCallType, PhoneBackend,
    PhoneCall, PhoneCallDirection, PhoneCallEnded, PhoneCallStatus, PhoneCommand, PhoneInbox, PhoneInitiate,
    PhoneState, PhoneStateSink,
  },
  dispatch::phone::PhoneDispatcher,
};
use bridgething_gateway::{HandlerError, PhoneHandler};
use libbridgething::{
  AcceptCallAction as WireAccept, CallEndReason as WireEndReason, CommunicationsState as WireCommunications,
  DtmfTone as WireTone, EndCallAction as WireEnd, InitiateCallType as WireInitiateKind, PhoneCall as WireCall,
  PhoneState as WireState,
  gateway::{
    GatewayToBridgeMsg, GatewayToBridgeMsgData, GatewayToBridgePhoneMsg, PhoneAcceptAction, PhoneCallAction,
    PhoneCallEnded as WireCallEnded, PhoneDtmfAction, PhoneEndAction, PhoneInitiateAction, PhoneMuteAction,
  },
  wire::WireError,
};

use crate::support::Peer;

fn call(id: &str) -> PhoneCall {
  PhoneCall {
    call_id: id.into(),
    remote_id: "+15550100".into(),
    display_name: "Ada".into(),
    status: PhoneCallStatus::Active,
    direction: PhoneCallDirection::Incoming,
    started_at_unix_s: None,
    label: None,
    address_book_id: None,
    service: None,
    is_conferenced: None,
    conference_group: None,
  }
}

#[derive(Default)]
struct FakePhone {
  abandon_state: bool,
  state: Mutex<PhoneState>,
  inbox: Mutex<Option<Arc<PhoneInbox>>>,
  commands: Mutex<Vec<PhoneCommand>>,
  stopped: AtomicUsize,
}

impl FakePhone {
  fn holding(calls: Vec<PhoneCall>) -> Arc<Self> {
    Arc::new(Self {
      state: Mutex::new(PhoneState { active_calls: calls }),
      ..Default::default()
    })
  }

  fn abandoning_state() -> Arc<Self> {
    Arc::new(Self {
      abandon_state: true,
      ..Default::default()
    })
  }

  fn post(&self, event: impl FnOnce(&PhoneInbox)) {
    if let Some(inbox) = self.inbox.lock().unwrap().as_ref() {
      event(inbox);
    }
  }
}

impl PhoneBackend for FakePhone {
  fn start(&self, inbox: Arc<PhoneInbox>) {
    *self.inbox.lock().unwrap() = Some(inbox);
  }

  fn stop(&self) {
    self.stopped.fetch_add(1, Ordering::SeqCst);
  }

  fn command(&self, cmd: PhoneCommand) {
    self.commands.lock().unwrap().push(cmd);
  }

  fn state_get(&self, sink: Arc<PhoneStateSink>) {
    if self.abandon_state {
      return;
    }
    sink.complete(self.state.lock().unwrap().clone());
  }
}

fn started(msg: &GatewayToBridgeMsg) -> Option<WireCall> {
  match &msg.data {
    GatewayToBridgeMsgData::Phone(GatewayToBridgePhoneMsg::CallStarted(call)) => Some(call.clone()),
    _ => None,
  }
}

fn updated(msg: &GatewayToBridgeMsg) -> Option<WireCall> {
  match &msg.data {
    GatewayToBridgeMsgData::Phone(GatewayToBridgePhoneMsg::CallUpdated(call)) => Some(call.clone()),
    _ => None,
  }
}

fn ended(msg: &GatewayToBridgeMsg) -> Option<WireCallEnded> {
  match &msg.data {
    GatewayToBridgeMsgData::Phone(GatewayToBridgePhoneMsg::CallEnded(ended)) => Some(ended.clone()),
    _ => None,
  }
}

fn snapshot(msg: &GatewayToBridgeMsg) -> Option<WireState> {
  match &msg.data {
    GatewayToBridgeMsgData::Phone(GatewayToBridgePhoneMsg::Snapshot(reply)) => Some(reply.state.clone()),
    _ => None,
  }
}

fn communications(msg: &GatewayToBridgeMsg) -> Option<WireCommunications> {
  match &msg.data {
    GatewayToBridgeMsgData::Phone(GatewayToBridgePhoneMsg::CommunicationsSnapshot(snapshot)) => {
      Some(snapshot.state.clone())
    }
    _ => None,
  }
}

async fn boot(backend: Arc<FakePhone>) -> (PhoneDispatcher, Peer) {
  let (gateway, peer) = Peer::link();
  let dispatcher = PhoneDispatcher::new(backend, Arc::new(gateway));
  dispatcher.start().await;
  (dispatcher, peer)
}

#[tokio::test(flavor = "multi_thread")]
async fn every_inbound_verb_reaches_the_backend_as_a_command() {
  let backend = FakePhone::holding(vec![]);
  let (dispatcher, _peer) = boot(backend.clone()).await;

  dispatcher
    .answer(PhoneCallAction { call_id: "c1".into() })
    .await
    .expect("accepted");
  dispatcher
    .accept(PhoneAcceptAction {
      call_id: "c2".into(),
      action: WireAccept::EndAndAccept,
    })
    .await
    .expect("accepted");
  dispatcher
    .decline(PhoneCallAction { call_id: "c3".into() })
    .await
    .expect("accepted");
  dispatcher
    .end(PhoneCallAction { call_id: "c4".into() })
    .await
    .expect("accepted");
  dispatcher
    .end_typed(PhoneEndAction {
      call_id: "c5".into(),
      action: WireEnd::EndAll,
    })
    .await
    .expect("accepted");
  dispatcher
    .hold(PhoneCallAction { call_id: "c6".into() })
    .await
    .expect("accepted");
  dispatcher
    .unhold(PhoneCallAction { call_id: "c7".into() })
    .await
    .expect("accepted");
  dispatcher
    .initiate(PhoneInitiateAction {
      kind: WireInitiateKind::Destination,
      destination_id: Some("+15551234".into()),
      service: None,
      address_book_id: None,
    })
    .await
    .expect("accepted");
  dispatcher.swap().await.expect("accepted");
  dispatcher.merge().await.expect("accepted");
  dispatcher.mute(PhoneMuteAction { mute: true }).await.expect("accepted");
  dispatcher
    .dtmf(PhoneDtmfAction {
      call_id: Some("c8".into()),
      tone: WireTone::Star,
    })
    .await
    .expect("accepted");

  assert_eq!(
    *backend.commands.lock().unwrap(),
    vec![
      PhoneCommand::Answer { call_id: "c1".into() },
      PhoneCommand::Accept {
        call_id: "c2".into(),
        action: AcceptCallAction::EndAndAccept,
      },
      PhoneCommand::Decline { call_id: "c3".into() },
      PhoneCommand::End { call_id: "c4".into() },
      PhoneCommand::EndTyped {
        call_id: "c5".into(),
        action: EndCallAction::EndAll,
      },
      PhoneCommand::Hold { call_id: "c6".into() },
      PhoneCommand::Unhold { call_id: "c7".into() },
      PhoneCommand::Initiate {
        action: PhoneInitiate {
          kind: InitiateCallType::Destination,
          destination_id: Some("+15551234".into()),
          service: None,
          address_book_id: None,
        },
      },
      PhoneCommand::Swap,
      PhoneCommand::Merge,
      PhoneCommand::Mute { muted: true },
      PhoneCommand::Dtmf {
        call_id: Some("c8".into()),
        tone: DtmfTone::Star,
      },
    ]
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn state_get_replies_with_the_backend_state() {
  let backend = FakePhone::holding(vec![call("c1")]);
  let (dispatcher, _peer) = boot(backend).await;

  let reply = dispatcher.state_get().await.expect("a state");

  assert_eq!(reply.response.state.active_calls.len(), 1);
  assert_eq!(reply.response.state.active_calls[0].call_id, "c1");
  assert_eq!(reply.response.state.active_calls[0].display_name, "Ada");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_backend_that_abandons_state_get_fails_the_request() {
  let backend = FakePhone::abandoning_state();
  let (dispatcher, _peer) = boot(backend).await;

  let refusal = dispatcher.state_get().await.expect_err("a refusal");

  match refusal {
    HandlerError::Wire(WireError::HandlerFailed { .. }) => {}
    other => panic!("an unanswerable request fails rather than parking the routing path, got {other:?}"),
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn backend_events_surface_as_wire_frames() {
  let backend = FakePhone::holding(vec![]);
  let (_dispatcher, peer) = boot(backend.clone()).await;
  let live = call("c9");

  backend.post(|inbox| inbox.on_call_started(live.clone()));
  backend.post(|inbox| inbox.on_call_updated(live.clone()));
  backend.post(|inbox| {
    inbox.on_state(PhoneState {
      active_calls: vec![live.clone()],
    })
  });
  backend.post(|inbox| {
    inbox.on_call_ended(PhoneCallEnded {
      call_id: "c9".into(),
      reason: CallEndReason::Remote,
    })
  });
  backend.post(|inbox| {
    inbox.on_communications(CommunicationsState {
      carrier_name: Some("Test Mobile".into()),
      ..Default::default()
    })
  });

  assert_eq!(peer.wait("a callStarted", started).await.call_id, "c9");
  assert_eq!(peer.wait("a callUpdated", updated).await.call_id, "c9");
  assert_eq!(peer.wait("a snapshot", snapshot).await.active_calls.len(), 1);
  assert_eq!(peer.wait("a callEnded", ended).await.reason, WireEndReason::Remote);
  assert_eq!(
    peer
      .wait("a communications snapshot", communications)
      .await
      .carrier_name,
    Some("Test Mobile".into())
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn announcing_seeds_the_peer_with_the_live_calls() {
  let backend = FakePhone::holding(vec![call("c1")]);
  let (dispatcher, peer) = boot(backend).await;

  peer.quiet("an unasked-for snapshot", snapshot).await;
  dispatcher.announce().await;

  let seeded = peer.wait("the announced snapshot", snapshot).await;
  assert_eq!(seeded.active_calls.len(), 1);
  assert_eq!(seeded.active_calls[0].call_id, "c1");
}

#[tokio::test(flavor = "multi_thread")]
async fn stopping_ends_the_relay() {
  let backend = FakePhone::holding(vec![]);
  let (dispatcher, peer) = boot(backend.clone()).await;

  backend.post(|inbox| inbox.on_call_started(call("c1")));
  peer.wait("the first callStarted", started).await;

  dispatcher.stop().await;
  backend.post(|inbox| inbox.on_call_started(call("c2")));

  assert_eq!(backend.stopped.load(Ordering::SeqCst), 1, "the backend is told to stop");
  assert_eq!(
    peer.settled_count(started).await,
    1,
    "a stopped dispatcher forwards nothing further"
  );
}
