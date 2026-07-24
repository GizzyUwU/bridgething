use std::time::Duration;

use bridgething::ClientMode;
use bridgething_test_harness::DeviceHarness;

#[tokio::test]
#[ignore = "requires a booted Car Thing with the test-tap daemon deployed"]
async fn t3_frame_tap_bridge_on_device() {
  let device = DeviceHarness::from_env().expect("device env (SUPERBIRD_HOST/SUPERBIRD_BT_MAC)");
  let mut frames = device.frame_tap().await.expect("frame-tap ws over the tunnel");

  let _client = device
    .connect_modern_client()
    .await
    .expect("modern client over usb gadget");

  let observed = frames
    .wait_for(Duration::from_secs(10), |f| f.mode == ClientMode::Modern)
    .await;
  assert!(
    observed.is_some(),
    "device frame-tap WS bridge never delivered the modern snapshot frame"
  );
}
