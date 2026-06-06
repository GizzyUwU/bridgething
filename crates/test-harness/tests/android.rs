//! Android (gateway-only) scenarios driven in-process against a headless
//! daemon. Each test assembles a fresh daemon, attaches a real companion
//! (the `bridgething-gateway` SDK) over a duplex-backed RFCOMM-shaped link,
//! drives the real wire surface, and asserts on merged daemon state.

use std::time::Duration;

use bridgething_test_harness::Harness;
use libbridgething::{
  CompanionAuthorityScope, GatewayCapabilities, GatewayInfo, MediaItem, PlayerState, gateway::AuthorityClaim,
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
