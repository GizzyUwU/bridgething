use std::{
  sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
  },
  time::Duration,
};

use bridgething_companion::{
  backend::{GeoAccuracy, GeoError, GeoInbox, GeoProvider, Position},
  dispatch::geo::GeoDispatcher,
};
use bridgething_gateway::{GeoHandler, HandlerError};
use libbridgething::{
  GeoAccuracy as WireAccuracy, GeoError as WireGeoError, Position as WirePosition,
  gateway::{GatewayToBridgeGeoMsg, GatewayToBridgeMsg, GatewayToBridgeMsgData, GeoErrorReply, GeoGetOnce, GeoWatch},
};

use crate::support::Peer;

const FIX: Position = Position {
  lat: 40.7128,
  lon: -74.0060,
  alt_m: None,
  accuracy_m: 8.0,
  speed_mps: None,
  heading_deg: None,
  ts_unix_s: 1_700_000_000,
};

#[derive(Default)]
struct FakeGeo {
  usable: AtomicBool,
  swallow_once: bool,
  inbox: Mutex<Option<Arc<GeoInbox>>>,
  next_error: Mutex<Option<GeoError>>,
  fail_on_start: Mutex<Option<GeoError>>,
  accuracies: Mutex<Vec<GeoAccuracy>>,
  authorizations: AtomicUsize,
  start_updating: AtomicUsize,
  stop_updating: AtomicUsize,
  cancel_once: AtomicUsize,
}

impl FakeGeo {
  fn new() -> Arc<Self> {
    let fake = Self::default();
    fake.usable.store(true, Ordering::SeqCst);
    Arc::new(fake)
  }

  fn unusable() -> Arc<Self> {
    Arc::new(Self::default())
  }

  fn silent() -> Arc<Self> {
    let fake = Self {
      swallow_once: true,
      ..Default::default()
    };
    fake.usable.store(true, Ordering::SeqCst);
    Arc::new(fake)
  }

  fn failing_once(error: GeoError) -> Arc<Self> {
    let fake = Self::new();
    *fake.next_error.lock().unwrap() = Some(error);
    fake
  }

  fn failing_watch(error: GeoError) -> Arc<Self> {
    let fake = Self::new();
    *fake.fail_on_start.lock().unwrap() = Some(error);
    fake
  }

  fn post(&self, event: impl FnOnce(&GeoInbox)) {
    if let Some(inbox) = self.inbox.lock().unwrap().as_ref() {
      event(inbox);
    }
  }

  fn revoke(&self) {
    self.usable.store(false, Ordering::SeqCst);
    self.post(|inbox| inbox.on_authorization_change(false));
  }
}

impl GeoProvider for FakeGeo {
  fn can_provide_location(&self) -> bool {
    self.usable.load(Ordering::SeqCst)
  }

  fn start(&self, inbox: Arc<GeoInbox>) {
    *self.inbox.lock().unwrap() = Some(inbox);
  }

  fn stop(&self) {
    *self.inbox.lock().unwrap() = None;
  }

  fn configure(&self, accuracy: GeoAccuracy) {
    self.accuracies.lock().unwrap().push(accuracy);
  }

  fn request_authorization(&self) {
    self.authorizations.fetch_add(1, Ordering::SeqCst);
  }

  fn start_updating(&self) {
    self.start_updating.fetch_add(1, Ordering::SeqCst);
    match self.fail_on_start.lock().unwrap().take() {
      Some(error) => self.post(|inbox| inbox.on_error(error)),
      None => self.post(|inbox| inbox.on_position(FIX)),
    }
  }

  fn stop_updating(&self) {
    self.stop_updating.fetch_add(1, Ordering::SeqCst);
  }

  fn request_once(&self) {
    if self.swallow_once {
      return;
    }
    match self.next_error.lock().unwrap().take() {
      Some(error) => self.post(|inbox| inbox.on_error(error)),
      None => self.post(|inbox| inbox.on_position(FIX)),
    }
  }

  fn cancel_once(&self) {
    self.cancel_once.fetch_add(1, Ordering::SeqCst);
  }
}

#[derive(Default)]
struct Grants(Mutex<Vec<bool>>);

impl Grants {
  fn sink(self: &Arc<Self>) -> Arc<dyn Fn(bool) + Send + Sync> {
    let held = self.clone();
    Arc::new(move |granted| held.0.lock().unwrap().push(granted))
  }

  fn seen(&self) -> Vec<bool> {
    self.0.lock().unwrap().clone()
  }
}

fn position(msg: &GatewayToBridgeMsg) -> Option<WirePosition> {
  match msg.data {
    GatewayToBridgeMsgData::Geo(GatewayToBridgeGeoMsg::Position(position)) => Some(position),
    _ => None,
  }
}

fn geo_error(msg: &GatewayToBridgeMsg) -> Option<WireGeoError> {
  match msg.data {
    GatewayToBridgeMsgData::Geo(GatewayToBridgeGeoMsg::ErrorEvent(GeoErrorReply { error })) => Some(error),
    _ => None,
  }
}

fn watch(accuracy: WireAccuracy) -> GeoWatch {
  GeoWatch {
    accuracy,
    min_interval_ms: 0,
  }
}

async fn boot(provider: Arc<FakeGeo>) -> (GeoDispatcher, Peer, Arc<Grants>) {
  let (gateway, peer) = Peer::link();
  let grants = Arc::new(Grants::default());
  let dispatcher = GeoDispatcher::new(provider, Arc::new(gateway), grants.sink());
  dispatcher.start().await;
  (dispatcher, peer, grants)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_one_shot_replies_with_the_providers_fix() {
  let provider = FakeGeo::new();
  let (dispatcher, _peer, _grants) = boot(provider.clone()).await;

  let reply = dispatcher
    .get_once(GeoGetOnce {
      accuracy: WireAccuracy::Fine,
    })
    .await
    .expect("a fix");

  assert_eq!(reply.response.position.lat, FIX.lat);
  assert_eq!(reply.response.position.lon, FIX.lon);
  assert_eq!(*provider.accuracies.lock().unwrap(), vec![GeoAccuracy::Fine]);
  assert_eq!(provider.authorizations.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_coarse_one_shot_asks_the_provider_for_coarse() {
  let provider = FakeGeo::new();
  let (dispatcher, _peer, _grants) = boot(provider.clone()).await;

  dispatcher
    .get_once(GeoGetOnce {
      accuracy: WireAccuracy::Coarse,
    })
    .await
    .expect("a fix");

  assert_eq!(*provider.accuracies.lock().unwrap(), vec![GeoAccuracy::Coarse]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_provider_error_becomes_the_one_shots_error_reply() {
  let provider = FakeGeo::failing_once(GeoError::PermissionDenied);
  let (dispatcher, _peer, _grants) = boot(provider).await;

  let refusal = dispatcher
    .get_once(GeoGetOnce {
      accuracy: WireAccuracy::Fine,
    })
    .await
    .expect_err("a refusal");

  assert_eq!(
    refusal,
    HandlerError::Domain(GeoErrorReply {
      error: WireGeoError::PermissionDenied
    })
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_silent_provider_times_the_one_shot_out_and_cancels_it() {
  let provider = FakeGeo::silent();
  let (gateway, _peer) = Peer::link();
  let grants = Arc::new(Grants::default());
  let dispatcher = GeoDispatcher::new(provider.clone(), Arc::new(gateway), grants.sink())
    .with_one_shot_timeout(Duration::from_millis(150));
  dispatcher.start().await;

  let refusal = dispatcher
    .get_once(GeoGetOnce {
      accuracy: WireAccuracy::Fine,
    })
    .await
    .expect_err("a refusal");

  assert_eq!(
    refusal,
    HandlerError::Domain(GeoErrorReply {
      error: WireGeoError::Unavailable
    })
  );
  assert_eq!(
    provider.cancel_once.load(Ordering::SeqCst),
    1,
    "the abandoned request is withdrawn from the provider"
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_one_shot_is_refused_when_location_is_definitively_unusable() {
  let provider = FakeGeo::unusable();
  let (dispatcher, _peer, _grants) = boot(provider.clone()).await;

  let refusal = dispatcher
    .get_once(GeoGetOnce {
      accuracy: WireAccuracy::Fine,
    })
    .await
    .expect_err("a refusal");

  assert_eq!(
    refusal,
    HandlerError::Domain(GeoErrorReply {
      error: WireGeoError::PermissionDenied
    })
  );
  assert_eq!(
    provider.accuracies.lock().unwrap().len(),
    0,
    "a refused request never reaches the provider"
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_watch_broadcasts_every_fix() {
  let provider = FakeGeo::new();
  let (dispatcher, peer, _grants) = boot(provider.clone()).await;

  dispatcher.watch(watch(WireAccuracy::Fine)).await.expect("accepted");

  let broadcast = peer.wait("a geo position", position).await;
  assert_eq!(broadcast.lat, FIX.lat);
  assert_eq!(*provider.accuracies.lock().unwrap(), vec![GeoAccuracy::Fine]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_watch_at_a_new_accuracy_restarts_the_platform_watch() {
  let provider = FakeGeo::new();
  let (dispatcher, _peer, _grants) = boot(provider.clone()).await;

  dispatcher.watch(watch(WireAccuracy::Fine)).await.expect("accepted");
  dispatcher.watch(watch(WireAccuracy::Coarse)).await.expect("accepted");

  assert_eq!(provider.stop_updating.load(Ordering::SeqCst), 1);
  assert_eq!(provider.start_updating.load(Ordering::SeqCst), 2);
  assert_eq!(
    *provider.accuracies.lock().unwrap(),
    vec![GeoAccuracy::Fine, GeoAccuracy::Coarse]
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_watch_at_the_same_accuracy_does_not_restart() {
  let provider = FakeGeo::new();
  let (dispatcher, _peer, _grants) = boot(provider.clone()).await;

  dispatcher.watch(watch(WireAccuracy::Fine)).await.expect("accepted");
  dispatcher.watch(watch(WireAccuracy::Fine)).await.expect("accepted");

  assert_eq!(provider.stop_updating.load(Ordering::SeqCst), 0);
  assert_eq!(provider.start_updating.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn unwatching_stops_the_provider_and_the_broadcast() {
  let provider = FakeGeo::new();
  let (dispatcher, peer, _grants) = boot(provider.clone()).await;

  dispatcher.watch(watch(WireAccuracy::Fine)).await.expect("accepted");
  peer.wait("a geo position", position).await;
  dispatcher.unwatch().await.expect("accepted");

  provider.post(|inbox| inbox.on_position(FIX));

  assert_eq!(provider.stop_updating.load(Ordering::SeqCst), 1);
  assert_eq!(
    peer.settled_count(position).await,
    1,
    "a fix arriving after unwatch is not broadcast"
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_watch_the_provider_refuses_emits_an_error_event() {
  let provider = FakeGeo::failing_watch(GeoError::PermissionDenied);
  let (dispatcher, peer, _grants) = boot(provider).await;

  dispatcher.watch(watch(WireAccuracy::Fine)).await.expect("accepted");

  assert_eq!(
    peer.wait("a geo error", geo_error).await,
    WireGeoError::PermissionDenied
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_watch_is_refused_when_location_is_definitively_unusable() {
  let provider = FakeGeo::unusable();
  let (dispatcher, peer, _grants) = boot(provider.clone()).await;

  dispatcher.watch(watch(WireAccuracy::Fine)).await.expect("accepted");

  assert_eq!(
    peer.wait("a geo error", geo_error).await,
    WireGeoError::PermissionDenied
  );
  assert_eq!(
    provider.start_updating.load(Ordering::SeqCst),
    0,
    "a refused watch never starts the radio"
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_starting_grant_is_reported_so_the_first_announce_is_honest() {
  let provider = FakeGeo::unusable();
  let (_dispatcher, _peer, grants) = boot(provider).await;

  assert_eq!(grants.seen(), vec![false]);
}

#[tokio::test(flavor = "multi_thread")]
async fn losing_authorization_is_reported_so_the_capability_can_be_revoked() {
  let provider = FakeGeo::new();
  let (_dispatcher, _peer, grants) = boot(provider.clone()).await;
  assert_eq!(grants.seen(), vec![true]);

  provider.revoke();

  let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
  while grants.seen() != vec![true, false] {
    assert!(
      tokio::time::Instant::now() < deadline,
      "the revoked grant was never reported, saw {:?}",
      grants.seen()
    );
    tokio::time::sleep(Duration::from_millis(5)).await;
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_fix_answering_a_one_shot_is_broadcast_too_while_watching() {
  let provider = FakeGeo::new();
  let (dispatcher, peer, _grants) = boot(provider.clone()).await;

  dispatcher.watch(watch(WireAccuracy::Fine)).await.expect("accepted");
  peer.wait("the watch's first position", position).await;

  dispatcher
    .get_once(GeoGetOnce {
      accuracy: WireAccuracy::Fine,
    })
    .await
    .expect("a fix");

  assert_eq!(
    peer.settled_count(position).await,
    2,
    "the fix that answered the request is still a fix the watcher asked to hear about"
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn stopping_ends_the_one_shots_in_flight() {
  let provider = FakeGeo::silent();
  let (gateway, _peer) = Peer::link();
  let grants = Arc::new(Grants::default());
  let dispatcher = Arc::new(GeoDispatcher::new(provider.clone(), Arc::new(gateway), grants.sink()));
  dispatcher.start().await;

  let pending = tokio::spawn({
    let dispatcher = dispatcher.clone();
    async move {
      dispatcher
        .get_once(GeoGetOnce {
          accuracy: WireAccuracy::Fine,
        })
        .await
    }
  });

  let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
  while provider.authorizations.load(Ordering::SeqCst) == 0 {
    assert!(
      tokio::time::Instant::now() < deadline,
      "the one-shot never reached the provider"
    );
    tokio::time::sleep(Duration::from_millis(5)).await;
  }

  dispatcher.stop().await;

  let refusal = tokio::time::timeout(Duration::from_secs(5), pending)
    .await
    .expect("the pending one-shot was answered rather than parked")
    .expect("the task ran")
    .expect_err("a refusal");
  assert_eq!(
    refusal,
    HandlerError::Domain(GeoErrorReply {
      error: WireGeoError::Unavailable
    })
  );
  assert_eq!(provider.cancel_once.load(Ordering::SeqCst), 1);
}
