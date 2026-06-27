//! Android (gateway-only) scenarios driven in-process against a headless
//! daemon. Each test assembles a fresh daemon, attaches a real companion
//! (the `bridgething-gateway` SDK) over a duplex-backed RFCOMM-shaped link,
//! drives the real wire surface, and asserts on merged daemon state.

use std::time::Duration;

use bluer::Address;
use bridgething_test_harness::Harness;
use libbridgething::{
  CompanionAuthorityScope, Device, DeviceType, GatewayCapabilities, GatewayInfo, MediaItem, PeerIap2Status,
  PlayerState, gateway::AuthorityClaim,
};

const CONVERGE: Duration = Duration::from_secs(3);

/// A minimal Android announce payload - enough for the daemon to register the
/// companion and mark it connected (the Android useful-link path).
fn caps() -> GatewayCapabilities {
  GatewayCapabilities {
    gateway: GatewayInfo {
      address: String::new(),
      name: "harness-android".into(),
      os_name: "android".into(),
      app_name: "harness-android".into(),
      app_version: "0.0.0".into(),
      adapter_version: "harness".into(),
      lib_version: "0.0.0".into(),
      libbridgething_version: format!("v{}", libbridgething::LIBBRIDGETHING_VERSION),
    },
    ..Default::default()
  }
}

/// A companion announces, claims metadata authority, and pushes a track.
/// Merged player state must reflect exactly the companion-supplied data.
#[tokio::test]
async fn gateway_only_now_playing() {
  let harness = Harness::start().await.expect("harness start");
  let phone = harness.connect_android().await.expect("connect");

  phone.capabilities().announce(caps()).await.expect("announce");
  phone
    .authority()
    .claim(AuthorityClaim {
      scope: CompanionAuthorityScope::NowPlayingMetadata,
      app_bundle: None,
    })
    .await
    .expect("claim metadata");
  phone
    .player()
    .snapshot(PlayerState {
      track: Some(MediaItem {
        persistent_id: Some("track-1".into()),
        title: Some("Test Song".into()),
        artist: Some("Test Artist".into()),
        album: Some("Test Album".into()),
        ..Default::default()
      }),
      ..Default::default()
    })
    .await
    .expect("now playing");

  let converged = harness
    .wait_for(
      |state| state.player.state_reply().state.track.and_then(|t| t.title).as_deref() == Some("Test Song"),
      CONVERGE,
    )
    .await;
  assert!(converged, "merged title never reflected companion data");

  let track = harness.state().player.state_reply().state.track.expect("track present");
  assert_eq!(track.title.as_deref(), Some("Test Song"));
  assert_eq!(track.artist.as_deref(), Some("Test Artist"));
  assert_eq!(track.album.as_deref(), Some("Test Album"));
}

/// Without an authority claim, companion metadata must NOT surface in
/// merged state - the merge is authority-gated, not "companion always
/// wins". This guards the invariant from the other direction.
#[tokio::test]
async fn no_authority_no_merge() {
  let harness = Harness::start().await.expect("harness start");
  let phone = harness.connect_android().await.expect("connect");

  phone.capabilities().announce(caps()).await.expect("announce");
  // deliberately NO claim_authority
  phone
    .player()
    .snapshot(PlayerState {
      track: Some(MediaItem {
        persistent_id: Some("track-2".into()),
        title: Some("Should Not Appear".into()),
        ..Default::default()
      }),
      ..Default::default()
    })
    .await
    .expect("now playing");

  // Give the daemon time to (not) apply it, then assert it never surfaced.
  let leaked = harness
    .wait_for(
      |state| state.player.state_reply().state.track.and_then(|t| t.title).as_deref() == Some("Should Not Appear"),
      Duration::from_millis(400),
    )
    .await;
  assert!(!leaked, "companion metadata surfaced without an authority claim");
}

/// The companion is the only authority source; when it disconnects, the
/// PeerTracker diff hook must drop the scopes it held. Dropping the mock
/// phone closes the duplex, which the daemon sees as a peer disconnect.
#[tokio::test]
async fn companion_disconnect_clears_authority() {
  let harness = Harness::start().await.expect("harness start");
  let phone = harness.connect_android().await.expect("connect");

  phone.capabilities().announce(caps()).await.expect("announce");

  // Wait until the daemon registers the companion as connected before
  // claiming/disconnecting. The daemon dispatches each inbound message
  // on its own task, so without this the disconnect can race ahead of
  // the announce and the companion-lost transition never fires.
  let connected = harness
    .wait_for(|state| state.capabilities.snapshot().gateway.is_some(), CONVERGE)
    .await;
  assert!(connected, "companion never registered as connected");

  phone
    .authority()
    .claim(AuthorityClaim {
      scope: CompanionAuthorityScope::NowPlayingMetadata,
      app_bundle: None,
    })
    .await
    .expect("claim metadata");
  phone
    .authority()
    .claim(AuthorityClaim {
      scope: CompanionAuthorityScope::NowPlayingPlayback,
      app_bundle: None,
    })
    .await
    .expect("claim playback");

  let claimed = harness
    .wait_for(
      |state| {
        state
          .authority
          .is_authoritative(CompanionAuthorityScope::NowPlayingMetadata)
      },
      CONVERGE,
    )
    .await;
  assert!(claimed, "authority claim never registered");

  // Disconnect: dropping the phone closes its half of the duplex, which
  // the daemon's reader observes as EOF and treats as a peer disconnect.
  drop(phone);

  let dropped = harness
    .wait_for(
      |state| {
        !state
          .authority
          .is_authoritative(CompanionAuthorityScope::NowPlayingMetadata)
          && !state
            .authority
            .is_authoritative(CompanionAuthorityScope::NowPlayingPlayback)
      },
      CONVERGE,
    )
    .await;
  assert!(dropped, "authority not cleared after companion disconnect");
}

/// A transient iAP2 control-link blip (the companion is already absent, the link warm-redials) must NOT
/// flap the stock connection indicator: the daemon debounces the useful-link-down. Driving the iap2 axis
/// directly is the only path that reaches the deferred branch - a companion-gateway drop is a functional
/// teardown shown immediately (see `companion_drop_shows_disconnected_promptly`).
#[tokio::test]
async fn iap2_control_blip_does_not_flap_stock_connection() {
  let harness = Harness::start().await.expect("harness start");

  let mac = Address([0xF0, 0x0D, 0xBE, 0xEF, 0x00, 0x01]);
  let device = Device {
    name: "test-iphone".into(),
    device_type: DeviceType::Ios,
    mac: mac.to_string(),
    default: false,
  };
  let peers = harness.state().peers.clone();
  peers.ensure_exists(mac, device).await;
  peers.set_iap2(mac, PeerIap2Status::Identified).await;
  let up = harness
    .wait_for(
      |s| s.peers.snapshot().peers.get(&mac).is_some_and(|p| p.has_useful_link()),
      CONVERGE,
    )
    .await;
  assert!(up, "iap2 peer never reached a useful link");

  // connect the stock client AFTER the link is up so its resync reflects connected (no spurious false).
  let mut stock = harness.connect_stock_client().await.expect("stock client");

  // control-link blip: drops to None then re-identifies well within the grace window.
  peers.set_iap2(mac, PeerIap2Status::None).await;
  peers.set_iap2(mac, PeerIap2Status::Identified).await;
  let back = harness
    .wait_for(
      |s| s.peers.snapshot().peers.get(&mac).is_some_and(|p| p.has_useful_link()),
      CONVERGE,
    )
    .await;
  assert!(back, "iap2 peer never re-reached a useful link");

  let mut flapped = false;
  while let Ok(Some(msg)) = tokio::time::timeout(Duration::from_millis(300), stock.recv()).await {
    if msg.contains("transport_connection_status") && msg.contains("false") {
      flapped = true;
      break;
    }
  }
  assert!(
    !flapped,
    "a transient iap2 blip pushed a stock disconnect (debounce regressed)"
  );
}

/// A companion-gateway drop is a functional teardown (authority/player/net-routes clear at once), so the
/// stock connection indicator must flip to disconnected promptly - NOT debounced like an iap2 blip.
#[tokio::test]
async fn companion_drop_shows_disconnected_promptly() {
  let harness = Harness::start().await.expect("harness start");
  let phone = harness.connect_android().await.expect("connect");
  phone.capabilities().announce(caps()).await.expect("announce");
  let up = harness
    .wait_for(
      |s| s.peers.snapshot().peers.values().any(|p| p.has_useful_link()),
      CONVERGE,
    )
    .await;
  assert!(up, "companion never reached a useful link");

  // connect the stock client AFTER the companion is up so its resync reflects connected.
  let mut stock = harness.connect_stock_client().await.expect("stock client");

  drop(phone);

  let mut disconnected = false;
  while let Ok(Some(msg)) = tokio::time::timeout(Duration::from_secs(2), stock.recv()).await {
    if msg.contains("transport_connection_status") && msg.contains("false") {
      disconnected = true;
      break;
    }
  }
  assert!(
    disconnected,
    "a companion drop must show disconnected promptly, not be debounced"
  );
}

fn ios_device(mac: Address) -> Device {
  Device {
    name: "test-iphone".into(),
    device_type: DeviceType::Ios,
    mac: mac.to_string(),
    default: false,
  }
}

/// BlueZ churns its Device1 object (a `DeviceRemoved` swiftly followed by a `DeviceAdded`) while the iAP2
/// session is still alive. The peer-removal that BlueZ implies must be deferred: it must not wipe the iap2
/// axis or flap the stock connection indicator, because the transport link never actually dropped.
#[tokio::test]
async fn bluez_device_churn_does_not_drop_iap2_peer() {
  let harness = Harness::start().await.expect("harness start");

  let mac = Address([0xF0, 0x0D, 0xBE, 0xEF, 0x00, 0x02]);
  let peers = harness.state().peers.clone();
  peers.ensure_exists(mac, ios_device(mac)).await;
  peers.set_iap2(mac, PeerIap2Status::Identified).await;
  let up = harness
    .wait_for(
      |s| s.peers.snapshot().peers.get(&mac).is_some_and(|p| p.has_useful_link()),
      CONVERGE,
    )
    .await;
  assert!(up, "iap2 peer never reached a useful link");

  let mut stock = harness.connect_stock_client().await.expect("stock client");

  // bluez Device1 churn: removed then re-added, with the iap2 session alive the whole time.
  peers.remove_bluez(mac).await;
  peers.upsert(mac, ios_device(mac)).await;

  let mut flapped = false;
  while let Ok(Some(msg)) = tokio::time::timeout(Duration::from_millis(300), stock.recv()).await {
    if msg.contains("transport_connection_status") && msg.contains("false") {
      flapped = true;
      break;
    }
  }
  assert!(
    !flapped,
    "a bluez Device1 churn pushed a stock disconnect (peer removal not deferred)"
  );

  let alive = harness
    .state()
    .peers
    .snapshot()
    .peers
    .get(&mac)
    .is_some_and(|p| p.has_useful_link());
  assert!(
    alive,
    "the iap2 peer was dropped by a bluez churn instead of surviving it"
  );
}

/// A genuine forget removes the BlueZ device AND drops the link. When the `DeviceRemoved` lands first (link
/// still alive) the removal is deferred; the subsequent link drop must complete it so no ghost peer lingers.
#[tokio::test]
async fn bluez_forget_completes_deferred_removal() {
  let harness = Harness::start().await.expect("harness start");

  let mac = Address([0xF0, 0x0D, 0xBE, 0xEF, 0x00, 0x03]);
  let peers = harness.state().peers.clone();
  peers.ensure_exists(mac, ios_device(mac)).await;
  peers.set_iap2(mac, PeerIap2Status::Identified).await;
  let up = harness
    .wait_for(
      |s| s.peers.snapshot().peers.get(&mac).is_some_and(|p| p.has_useful_link()),
      CONVERGE,
    )
    .await;
  assert!(up, "iap2 peer never reached a useful link");

  // forget: DeviceRemoved arrives while the link is still up (deferred), then the link drops.
  peers.remove_bluez(mac).await;
  peers.set_iap2(mac, PeerIap2Status::None).await;

  let gone = harness
    .wait_for(|s| s.peers.snapshot().peers.get(&mac).is_none(), CONVERGE)
    .await;
  assert!(
    gone,
    "a forget left a ghost peer: the deferred removal never completed on link drop"
  );
}
