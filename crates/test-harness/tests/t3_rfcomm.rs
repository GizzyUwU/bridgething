//! Tier-3 over-the-air scenarios against a booted Car Thing with the test-tap
//! daemon deployed. `#[ignore]` because they need hardware: a flashed device
//! reachable over the USB-gadget network and a host BT radio. Address comes
//! from `SUPERBIRD_BT_MAC` (and `SUPERBIRD_HOST` if not `bridgething.local`).
//!
//! Run: `cargo test -p bridgething-test-harness --test t3_rfcomm -- --ignored --nocapture`

use std::time::Duration;

use bridgething::ClientMode;
use bridgething_test_harness::DeviceHarness;
use libbridgething::{
  CompanionAuthorityScope, GatewayCapabilities, GatewayInfo, MediaItemUpdate, NowPlayingUpdate, gateway::AuthorityClaim,
};

fn caps() -> GatewayCapabilities {
  GatewayCapabilities {
    gateway: GatewayInfo {
      address: String::new(),
      name: "t3-over-air".into(),
      os_name: "android".into(),
      app_name: "t3-over-air".into(),
      app_version: "0.0.0".into(),
      adapter_version: "harness".into(),
      lib_version: "0.0.0".into(),
      libbridgething_version: format!("v{}", libbridgething::LIBBRIDGETHING_VERSION),
    },
    ..Default::default()
  }
}

/// Observation half on real hardware, no radio. A modern client connected over
/// the USB-gadget network is sent a capabilities snapshot; the device's frame-tap
/// WS bridge must deliver that egress frame to the host over the same tunnel.
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

/// The full driver half: announce + push now-playing over a real RFCOMM dial
/// (host radio -> the device's BCM chip -> bluez SPP), observed through the
/// tunneled frame-tap. Proves the radio path end to end against the new daemon.
#[tokio::test]
#[ignore = "requires a booted Car Thing with the test-tap daemon + a host BT radio"]
async fn t3_over_air_now_playing_reaches_frame_tap() {
  let device = DeviceHarness::from_env().expect("device env (SUPERBIRD_HOST/SUPERBIRD_BT_MAC)");
  let mut frames = device.frame_tap().await.expect("frame-tap ws over the tunnel");
  // a connected client gives the now-playing broadcast a recipient to egress to.
  let _client = device
    .connect_modern_client()
    .await
    .expect("modern client over usb gadget");

  let phone = device
    .connect_over_air()
    .await
    .expect("rfcomm dial over the real radio");
  phone.capabilities().announce(caps()).await.expect("announce");
  phone
    .authority()
    .claim(AuthorityClaim {
      scope: CompanionAuthorityScope::NowPlayingMetadata,
    })
    .await
    .expect("claim metadata authority");
  phone
    .player()
    .delta(NowPlayingUpdate {
      media_item: Some(MediaItemUpdate {
        persistent_id: Some("t3-track".into()),
        title: Some("T3 Over Air".into()),
        artist: Some("Real Radio".into()),
        ..Default::default()
      }),
      playback: None,
    })
    .await
    .expect("push now-playing over the air");

  let observed = frames
    .wait_for(Duration::from_secs(15), |f| f.json().contains("T3 Over Air"))
    .await;
  assert!(
    observed.is_some(),
    "over-air now-playing never reached the frame-tap over the tunnel"
  );
}
