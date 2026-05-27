//! Tier-3 harness-infra proof against a booted Car Thing with the test-tap
//! daemon deployed. Tier-local by nature (it asserts the rig itself works, not
//! daemon behavior), so it does not lift across tiers. `#[ignore]` because it
//! needs hardware: a flashed device reachable over the USB-gadget network.
//! Address comes from `SUPERBIRD_HOST` (default `bridgething.local`).
//!
//! The over-air daemon-behavior scenarios live in `seam.rs`, lifted across
//! tiers from one body.
//!
//! Run: `cargo test -p bridgething-test-harness --test t3_infra -- --ignored --nocapture`

use std::time::Duration;

use bridgething::ClientMode;
use bridgething_test_harness::DeviceHarness;

/// The frame-tap WS bridge works over the tunnel with no radio: a modern client
/// connected over the USB-gadget network is sent a capabilities snapshot, and
/// the device's frame-tap bridge must deliver that egress frame to the host.
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
