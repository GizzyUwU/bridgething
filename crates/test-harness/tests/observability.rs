use std::time::Duration;

use bridgething::ClientMode;
use bridgething_test_harness::Harness;

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

#[tokio::test]
async fn frame_tap_observes_stock_client_traffic() {
  use bridgething_iap2::{
    SessionEvent,
    csm::now_playing::{MediaItemAttributes, NowPlayingUpdate},
  };

  let harness = Harness::start().await.expect("harness start");
  let mut frames = harness.observe_frames();
  let _client = harness.connect_stock_client().await.expect("connect stock client");

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
