//! Egress frame-tap smoke tests. These prove the observer half of the rig:
//! a real client connects to the headless daemon's bound port and the
//! frame-tap mirrors what the daemon sends it. The flicker bug is a
//! broadcast-stream symptom, so observing the egress stream is the point.

use std::time::Duration;

use bridgething::ClientMode;
use bridgething_test_harness::Harness;

/// A new modern client is proactively sent a capabilities snapshot. The
/// frame observer, started before the connect, must observe that frame.
#[tokio::test]
async fn frame_tap_observes_new_modern_client_snapshot() {
  let harness = Harness::start().await.expect("harness start");
  let mut frames = harness.observe_frames();

  let _client = harness.connect_modern_client().await.expect("connect modern client");

  let observed = frames
    .wait_for(Duration::from_secs(2), |f| f.mode == ClientMode::Modern)
    .await
    .expect("frame-tap should observe a modern frame before timeout");

  assert_eq!(observed.mode, ClientMode::Modern);
}

/// The same modern-snapshot scenario, observed through the frame-tap WS bridge
/// instead of the in-process broadcast. Proves the bridge is honest: the daemon
/// serializes a `TappedFrame`, ships it over the WS, the host deserializes the
/// identical type, and `FrameObserver` sees it exactly as the in-process tap
/// would. This is the observation transport a device rig uses over the tunnel.
#[tokio::test]
async fn frame_tap_ws_bridge_mirrors_in_process_tap() {
  let harness = Harness::start().await.expect("harness start");
  let mut frames = harness.connect_frame_tap_ws().await.expect("connect frame-tap ws");

  let _client = harness.connect_modern_client().await.expect("connect modern client");

  let observed = frames
    .wait_for(Duration::from_secs(3), |f| f.mode == ClientMode::Modern)
    .await
    .expect("frame-tap WS bridge should deliver the modern snapshot frame");

  assert_eq!(observed.mode, ClientMode::Modern);
  assert!(
    !observed.json().is_empty(),
    "bridged frame should carry the serialized payload"
  );
}

/// The daemon broadcasts stock-translated now-playing to a bare stock client
/// like any other - no SPA request loop needed. Proves connect_stock_client
/// + that the merge/re-broadcast suspects are observable on the stock lane.
#[tokio::test]
async fn frame_tap_observes_stock_client_traffic() {
  use bridgething_iap2::{
    SessionEvent,
    csm::now_playing::{MediaItemAttributes, NowPlayingUpdate},
  };

  let harness = Harness::start().await.expect("harness start");
  let mut frames = harness.observe_frames();
  let _client = harness.connect_stock_client().await.expect("connect stock client");

  // barrier: a stock client gets no proactive frame on connect, so wait for
  // it to register before the broadcast or it can race ahead of it.
  let registered = harness
    .wait_for(|state| state.client_man.client_count() >= 1, Duration::from_secs(3))
    .await;
  assert!(registered, "stock client never registered");

  let phone = harness.iap2_peer();

  harness
    .inject_iap2(
      phone,
      SessionEvent::NowPlayingUpdate(NowPlayingUpdate {
        media_item: Some(MediaItemAttributes {
          persistent_id: Some(0x55),
          title: Some("Stock Song".into()),
          ..Default::default()
        }),
        playback: None,
      }),
    )
    .await
    .expect("inject iap2 now-playing");

  let observed = frames
    .wait_for(Duration::from_secs(3), |f| f.mode == ClientMode::Stock)
    .await
    .expect("frame-tap should observe a stock-translated frame");

  assert_eq!(observed.mode, ClientMode::Stock);
}
