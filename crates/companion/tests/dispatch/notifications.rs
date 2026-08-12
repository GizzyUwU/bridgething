use std::sync::{
  Arc, Mutex,
  atomic::{AtomicBool, AtomicUsize, Ordering},
};

use bridgething_companion::{
  backend::{
    ActionSink, DismissReason, NotificationActionError, NotificationApp, NotificationBackend, NotificationCategory,
    NotificationFlags, NotificationInbox, NotificationRemoved, WireNotification,
  },
  dispatch::notifications::NotificationDispatcher,
};
use libbridgething::{
  Notification, NotificationsError,
  gateway::{
    GatewayToBridgeMsg, GatewayToBridgeMsgData, GatewayToBridgeNotificationsMsg, NotificationInvoke,
    NotificationRemoved as WireRemoved,
  },
};

use crate::{poll::eventually, support::Peer};

#[derive(Default)]
struct FakeNotifications {
  refuse: Option<NotificationActionError>,
  abandon: bool,
  inbox: Mutex<Option<Arc<NotificationInbox>>>,
  positive: Mutex<Vec<String>>,
  negative: Mutex<Vec<String>>,
  stopped: AtomicUsize,
}

impl FakeNotifications {
  fn new() -> Arc<Self> {
    Arc::new(Self::default())
  }

  fn refusing(error: NotificationActionError) -> Arc<Self> {
    Arc::new(Self {
      refuse: Some(error),
      ..Default::default()
    })
  }

  fn abandoning() -> Arc<Self> {
    Arc::new(Self {
      abandon: true,
      ..Default::default()
    })
  }

  fn post(&self, event: impl FnOnce(&NotificationInbox)) {
    if let Some(inbox) = self.inbox.lock().unwrap().as_ref() {
      event(inbox);
    }
  }

  fn settle(&self, sink: Arc<ActionSink>) {
    if self.abandon {
      return;
    }
    match self.refuse.clone() {
      Some(error) => sink.fail(error),
      None => sink.complete(),
    }
  }
}

impl NotificationBackend for FakeNotifications {
  fn start(&self, inbox: Arc<NotificationInbox>) {
    *self.inbox.lock().unwrap() = Some(inbox);
  }

  fn stop(&self) {
    self.stopped.fetch_add(1, Ordering::SeqCst);
  }

  fn invoke_positive(&self, id: String, sink: Arc<ActionSink>) {
    self.positive.lock().unwrap().push(id);
    self.settle(sink);
  }

  fn invoke_negative(&self, id: String, sink: Arc<ActionSink>) {
    self.negative.lock().unwrap().push(id);
    self.settle(sink);
  }
}

fn notification(id: &str) -> WireNotification {
  WireNotification {
    id: id.into(),
    app: NotificationApp {
      bundle_id: "com.example".into(),
      display_name: Some("Example".into()),
      icon_asset_id: None,
    },
    category: NotificationCategory::Other,
    title: Some(format!("Title {id}")),
    subtitle: None,
    message: Some("Body".into()),
    timestamp_unix_s: Some(0),
    flags: NotificationFlags {
      silent: false,
      important: false,
    },
    positive_action: None,
    negative_action: None,
  }
}

fn posted(msg: &GatewayToBridgeMsg) -> Option<Notification> {
  match &msg.data {
    GatewayToBridgeMsgData::Notifications(GatewayToBridgeNotificationsMsg::Posted(notification)) => {
      Some(notification.clone())
    }
    _ => None,
  }
}

fn removed(msg: &GatewayToBridgeMsg) -> Option<WireRemoved> {
  match &msg.data {
    GatewayToBridgeMsgData::Notifications(GatewayToBridgeNotificationsMsg::Removed(removed)) => Some(removed.clone()),
    _ => None,
  }
}

fn notifications_error(msg: &GatewayToBridgeMsg) -> Option<NotificationsError> {
  match &msg.data {
    GatewayToBridgeMsgData::Notifications(GatewayToBridgeNotificationsMsg::ErrorEvent(reply)) => {
      Some(reply.error.clone())
    }
    _ => None,
  }
}

fn always_on() -> Arc<dyn Fn() -> bool + Send + Sync> {
  Arc::new(|| true)
}

async fn boot(
  backend: Arc<FakeNotifications>,
  enabled: Arc<dyn Fn() -> bool + Send + Sync>,
) -> (NotificationDispatcher, Peer) {
  let (gateway, peer) = Peer::link();
  let dispatcher = NotificationDispatcher::new(backend, Arc::new(gateway), enabled);
  dispatcher.start().await;
  (dispatcher, peer)
}

#[tokio::test(flavor = "multi_thread")]
async fn invokes_route_to_the_backend_with_the_id() {
  let backend = FakeNotifications::new();
  let (dispatcher, _peer) = boot(backend.clone(), always_on()).await;

  dispatcher
    .invoke_positive(NotificationInvoke { id: "n-1".into() })
    .await
    .expect("accepted");
  dispatcher
    .invoke_negative(NotificationInvoke { id: "n-2".into() })
    .await
    .expect("accepted");

  assert!(
    eventually(|| *backend.positive.lock().unwrap() == ["n-1"] && *backend.negative.lock().unwrap() == ["n-2"]).await,
    "both invokes reached the backend"
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_invoke_reports_an_error_event() {
  let backend = FakeNotifications::refusing(NotificationActionError::NotFound { id: "n-gone".into() });
  let (dispatcher, peer) = boot(backend, always_on()).await;

  dispatcher
    .invoke_positive(NotificationInvoke { id: "n-gone".into() })
    .await
    .expect("accepted");

  assert_eq!(
    peer.wait("a notifications error", notifications_error).await,
    NotificationsError::NotFound { id: "n-gone".into() }
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_invoke_the_backend_abandons_still_reports() {
  let backend = FakeNotifications::abandoning();
  let (dispatcher, peer) = boot(backend, always_on()).await;

  dispatcher
    .invoke_positive(NotificationInvoke { id: "n-1".into() })
    .await
    .expect("accepted");

  match peer.wait("a notifications error", notifications_error).await {
    NotificationsError::ActionRejected { .. } => {}
    other => panic!("an unanswered invoke is a refusal the webapp can act on, got {other:?}"),
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_posted_notification_relays_to_the_peer() {
  let backend = FakeNotifications::new();
  let (_dispatcher, peer) = boot(backend.clone(), always_on()).await;

  backend.post(|inbox| inbox.on_posted(notification("p-1")));

  assert_eq!(peer.wait("a posted notification", posted).await.id, "p-1");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_removed_notification_relays_to_the_peer() {
  let backend = FakeNotifications::new();
  let (_dispatcher, peer) = boot(backend.clone(), always_on()).await;

  backend.post(|inbox| {
    inbox.on_removed(NotificationRemoved {
      id: "r-1".into(),
      reason: DismissReason::RemoteDismissed,
    })
  });

  let gone = peer.wait("a removed notification", removed).await;
  assert_eq!(gone.id, "r-1");
  assert_eq!(gone.reason, libbridgething::DismissReason::RemoteDismissed);
}

#[tokio::test(flavor = "multi_thread")]
async fn connecting_does_not_backfill_the_shade() {
  let backend = FakeNotifications::new();
  let (_dispatcher, peer) = boot(backend.clone(), always_on()).await;

  peer.quiet("a backfilled notification", posted).await;

  backend.post(|inbox| inbox.on_posted(notification("live-1")));
  assert_eq!(peer.wait("the live notification", posted).await.id, "live-1");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_posted_notification_is_dropped_while_the_capability_is_off() {
  let backend = FakeNotifications::new();
  let (_dispatcher, peer) = boot(backend.clone(), Arc::new(|| false)).await;

  backend.post(|inbox| inbox.on_posted(notification("off-1")));

  peer.quiet("a notification the peer never asked for", posted).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_capability_gate_is_read_live() {
  let backend = FakeNotifications::new();
  let open = Arc::new(AtomicBool::new(false));
  let gate = open.clone();
  let (_dispatcher, peer) = boot(backend.clone(), Arc::new(move || gate.load(Ordering::SeqCst))).await;

  backend.post(|inbox| inbox.on_posted(notification("off-1")));
  peer.quiet("a notification posted while the cap was off", posted).await;

  open.store(true, Ordering::SeqCst);
  backend.post(|inbox| inbox.on_posted(notification("on-1")));
  assert_eq!(peer.wait("the notification posted after", posted).await.id, "on-1");
}

#[tokio::test(flavor = "multi_thread")]
async fn stopping_ends_the_relay() {
  let backend = FakeNotifications::new();
  let (dispatcher, peer) = boot(backend.clone(), always_on()).await;

  backend.post(|inbox| inbox.on_posted(notification("p-1")));
  peer.wait("the first notification", posted).await;

  dispatcher.stop().await;
  backend.post(|inbox| inbox.on_posted(notification("p-2")));

  assert_eq!(backend.stopped.load(Ordering::SeqCst), 1, "the backend is told to stop");
  assert_eq!(
    peer.settled_count(posted).await,
    1,
    "a stopped dispatcher forwards nothing further"
  );
}
