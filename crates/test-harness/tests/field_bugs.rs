use std::time::Duration;

use bridgething_iap2::csm::now_playing::{
  MediaItemAttributes as Iap2MediaItem, NowPlayingUpdate as Iap2NowPlaying, PlaybackAttributes, PlaybackState,
};
#[cfg(target_os = "linux")]
use bridgething_test_harness::DeviceHarness;
use bridgething_test_harness::{Harness, Iap2Source, Iap2SourceDriver};
use serde_json::Value;

#[tokio::test]
async fn stock_player_state_has_no_bridgething_leak() {
  let harness = Harness::start().await.expect("harness start");
  let mut frames = harness.observe_frames();

  let _modern = harness.connect_modern_client().await.expect("modern client");
  let registered = harness
    .wait_for(|s| s.client_man.client_count() >= 1, Duration::from_secs(5))
    .await;
  assert!(registered, "modern client never registered");

  let source = harness.iap2_source().await.expect("iap2 source");
  source
    .push_now_playing(Iap2NowPlaying {
      media_item: Some(Iap2MediaItem {
        persistent_id: Some(0x1234),
        title: Some("Real Track".into()),
        duration_ms: Some(200_000),
        ..Default::default()
      }),
      playback: Some(PlaybackAttributes {
        state: Some(PlaybackState::Playing),
        position_ms: Some(0),
        ..Default::default()
      }),
    })
    .await
    .expect("push now-playing");

  let modern_frame = frames
    .wait_for(Duration::from_secs(5), |f| f.json().contains("Real Track"))
    .await
    .expect("modern broadcast never carried the pushed track");
  assert_modern_frame_has_no_placeholder_leak(modern_frame.json());

  let mut stock = harness.connect_stock_client().await.expect("stock client");
  let request = serde_json::json!({
    "msgId": 1,
    "method": "com.spotify.superbird.player_state",
    "args": {},
    "userAction": false,
  });
  stock
    .send_text(request.to_string())
    .await
    .expect("send player_state request");

  let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
  let mut player_state_reply: Option<Value> = None;
  while tokio::time::Instant::now() < deadline {
    let remaining = deadline - tokio::time::Instant::now();
    let frame = match tokio::time::timeout(remaining, stock.recv()).await {
      Ok(Some(text)) => text,
      Ok(None) | Err(_) => break,
    };
    let Ok(value) = serde_json::from_str::<Value>(&frame) else {
      continue;
    };
    if value.get("type") == Some(&Value::String("com.spotify.superbird.player_state".to_string())) {
      player_state_reply = Some(value);
      break;
    }
  }

  let reply = player_state_reply.expect("stock player_state reply never arrived");
  let serialized = reply.to_string();
  assert!(
    !serialized.contains("BridgeThing"),
    "stock player_state leaked the literal 'BridgeThing' placeholder: {serialized}"
  );
  assert!(
    !serialized.contains("Thing Labs") && !serialized.contains("ThingLabs"),
    "stock player_state leaked the literal 'Thing Labs' placeholder: {serialized}"
  );
  assert!(
    !serialized.contains("bridgething:album:bridgething") && !serialized.contains("bridgething:artist:bridgething"),
    "stock player_state leaked the placeholder album/artist ids: {serialized}"
  );
}

fn assert_modern_frame_has_no_placeholder_leak(json: &str) {
  assert!(
    !json.contains("BridgeThing"),
    "modern player broadcast leaked the literal 'BridgeThing' placeholder: {json}"
  );
  assert!(
    !json.contains("Thing Labs") && !json.contains("ThingLabs"),
    "modern player broadcast leaked the literal 'Thing Labs' placeholder: {json}"
  );
  assert!(
    !json.contains("bridgething:album:bridgething") && !json.contains("bridgething:artist:bridgething"),
    "modern player broadcast leaked placeholder album/artist ids: {json}"
  );
}

#[tokio::test]
async fn forward_progress_under_load() {
  let harness = Harness::start().await.expect("harness start");
  let mut frames = harness.observe_frames();
  let _client = harness.connect_modern_client().await.expect("modern client");

  let registered = harness
    .wait_for(|s| s.client_man.client_count() >= 1, Duration::from_secs(5))
    .await;
  assert!(registered, "modern client never registered");

  let stock_noise = tokio::spawn({
    let mut stock = harness.connect_stock_client().await.expect("stock client");
    async move {
      for n in 0u64.. {
        let req = serde_json::json!({
          "msgId": n,
          "method": "com.spotify.superbird.player_state",
          "args": {},
          "userAction": false,
        });
        if stock.send_text(req.to_string()).await.is_err() {
          break;
        }
        let _ = tokio::time::timeout(Duration::from_millis(5), stock.recv()).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
      }
    }
  });

  let source = harness.iap2_source().await.expect("iap2 source");
  const N: u64 = 60;
  const PER_DELTA_DEADLINE: Duration = Duration::from_secs(1);

  for n in 0..N {
    let marker = format!("LoadStress-{n:03}");
    let pid = 0x10000 + n;
    source
      .push_now_playing(Iap2NowPlaying {
        media_item: Some(Iap2MediaItem {
          persistent_id: Some(pid),
          title: Some(marker.clone()),
          ..Default::default()
        }),
        playback: Some(PlaybackAttributes {
          state: Some(PlaybackState::Playing),
          position_ms: Some(((n * 137) % 200_000) as u32),
          ..Default::default()
        }),
      })
      .await
      .expect("push now-playing");

    let observed = frames
      .wait_for(PER_DELTA_DEADLINE, |f| f.json().contains(&marker))
      .await;
    assert!(
      observed.is_some(),
      "delta {n}/{N} marker '{marker}' did not reach the frame tap within {PER_DELTA_DEADLINE:?} - the daemon stalled mid-load",
    );

    tokio::time::sleep(Duration::from_millis(10)).await;
  }

  stock_noise.abort();
}

#[cfg(target_os = "linux")]
#[tokio::test]
#[ignore = "requires a booted Car Thing with the test-tap daemon + a host BT radio"]
async fn slow_art_under_baseline() {
  init_test_tracing();
  let device = DeviceHarness::from_env().expect("device env (SUPERBIRD_BT_MAC)");
  let mut frames = device.frame_tap().await.expect("frame tap over the tunnel");
  let _client = device.connect_modern_client().await.expect("modern client");

  let source = device.iap2_source().await.expect("iap2 emulator source");

  for (label, transfer_id, size) in [
    ("1KB", 11u8, 1_024usize),
    ("64KB", 13u8, 64 * 1024),
    ("250KB", 17u8, 250_291),
  ] {
    let pid = 0xC0FFEE00u64 + u64::from(transfer_id);
    let art_id = format!("iap2/art/{pid:016x}/{transfer_id}");
    source
      .push_now_playing(Iap2NowPlaying {
        media_item: Some(Iap2MediaItem {
          persistent_id: Some(pid),
          title: Some(format!("Slow Art {label}")),
          artwork_id: Some(transfer_id),
          ..Default::default()
        }),
        playback: None,
      })
      .await
      .expect("push now-playing");

    let bytes = vec![0xC3u8; size];
    let started = tokio::time::Instant::now();
    source.push_artwork(transfer_id, bytes).await.expect("push artwork");

    let observed = frames
      .wait_for(Duration::from_secs(12), |f| f.json().contains(&art_id))
      .await;
    let elapsed = started.elapsed();
    eprintln!(
      "[slow_art] {label} ({size} B): {elapsed:?} -> arrived={}",
      observed.is_some()
    );
    assert!(
      observed.is_some(),
      "{label} ({size} B) artwork never reached the frame-tap (elapsed {elapsed:?})"
    );
  }
}

#[cfg(target_os = "linux")]
#[tokio::test]
#[ignore = "requires a booted Car Thing + host BT radio; measure device journald rejects around it"]
async fn link_integrity_volume() {
  init_test_tracing();
  let device = DeviceHarness::from_env().expect("device env (SUPERBIRD_BT_MAC)");
  let _client = device.connect_modern_client().await.expect("modern client");
  let source = device.iap2_source().await.expect("iap2 emulator source");

  let rejects_before = device_checksum_rejects();
  let rx_before = device_hci_rx_bytes();

  let blob = vec![0xC3u8; 512 * 1024];
  let run_start = tokio::time::Instant::now();
  let mut total = 0usize;
  for i in 0..16u8 {
    let pid = 0xD00D_0000u64 + u64::from(i);
    source
      .push_now_playing(Iap2NowPlaying {
        media_item: Some(Iap2MediaItem {
          persistent_id: Some(pid),
          title: Some(format!("Integrity {i}")),
          artwork_id: Some(i),
          ..Default::default()
        }),
        playback: None,
      })
      .await
      .expect("push now-playing");
    source.push_artwork(i, blob.clone()).await.expect("push artwork");
    total += blob.len();
    tokio::time::sleep(Duration::from_secs(3)).await;
    eprintln!("[integrity] blob {i}: 512 KB ({} KB cumulative)", total / 1024);
  }
  tokio::time::sleep(Duration::from_secs(8)).await;

  let rejects_after = device_checksum_rejects();
  let rx_after = device_hci_rx_bytes();
  let rejects = rejects_after.saturating_sub(rejects_before);
  let rx_grew = rx_after.saturating_sub(rx_before);
  eprintln!(
    "[integrity] pushed {} KB over {:?}; device RX +{} KB, checksum rejects +{}",
    total / 1024,
    run_start.elapsed(),
    rx_grew / 1024,
    rejects
  );

  assert!(
    rx_grew >= 2 * 1024 * 1024,
    "only {} KB reached the device - too little to test link integrity (art pulls did not run?)",
    rx_grew / 1024
  );
  assert_eq!(
    rejects,
    0,
    "iap2 decoder rejected {rejects} inbound frames on bad payload checksum over {} KB - UART link is corrupting",
    rx_grew / 1024
  );
}

#[cfg(target_os = "linux")]
fn device_ssh(cmd: &str) -> String {
  let host = std::env::var("SUPERBIRD_HOST").unwrap_or_else(|_| "bridgething.local".into());
  let out = std::process::Command::new("ssh")
    .args([
      "-o",
      "AddressFamily=inet",
      "-o",
      "UserKnownHostsFile=/dev/null",
      "-o",
      "StrictHostKeyChecking=no",
      "-o",
      "ConnectTimeout=5",
      "-o",
      "LogLevel=ERROR",
      &format!("root@{host}"),
      cmd,
    ])
    .output()
    .expect("ssh to device");
  assert!(
    out.status.success(),
    "device ssh `{cmd}` failed: {}",
    String::from_utf8_lossy(&out.stderr)
  );
  String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[cfg(target_os = "linux")]
fn device_checksum_rejects() -> u64 {
  device_ssh("journalctl -u bridgething -b --no-pager | grep -c 'bad payload checksum' || true")
    .parse()
    .unwrap_or(0)
}

#[cfg(target_os = "linux")]
fn device_hci_rx_bytes() -> u64 {
  device_ssh("hciconfig hci0 | grep -oE 'RX bytes:[0-9]+' | head -1")
    .trim_start_matches("RX bytes:")
    .trim()
    .parse()
    .unwrap_or(0)
}

#[cfg(target_os = "linux")]
fn init_test_tracing() {
  use tracing_subscriber::EnvFilter;
  let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("bridgething_iap2=debug,info"));
  let _ = tracing_subscriber::fmt()
    .with_env_filter(filter)
    .with_test_writer()
    .try_init();
}

#[cfg(target_os = "linux")]
#[tokio::test]
#[ignore = "requires a booted Car Thing with the test-tap daemon + a host BT radio"]
async fn flood_iap2_and_companion_concurrent() {
  use bridgething_gateway::Gateway;
  use libbridgething::{
    CompanionAuthorityScope, GatewayCapabilities, GatewayInfo, MediaItem, Playback, PlayerState,
    gateway::AuthorityClaim,
  };

  init_test_tracing();
  let device = DeviceHarness::from_env().expect("device env (SUPERBIRD_BT_MAC)");
  let mut frames = device.frame_tap().await.expect("frame tap");
  let _modern = device.connect_modern_client().await.expect("modern client over usb");

  let iap2 = device.iap2_source().await.expect("iap2 emulator source");
  let gateway: Gateway = device
    .connect_over_air_extra()
    .await
    .expect("companion gateway over rfcomm (extra ACL)");

  let caps = GatewayCapabilities {
    gateway: GatewayInfo {
      address: String::new(),
      name: "flooder".into(),
      os_name: "android".into(),
      app_name: "flooder".into(),
      app_version: "0.0.0".into(),
      adapter_version: "harness".into(),
      lib_version: "0.0.0".into(),
      libbridgething_version: format!("v{}", libbridgething::LIBBRIDGETHING_VERSION),
    },
    ..Default::default()
  };
  gateway.capabilities().announce(caps).await.expect("announce");
  gateway
    .authority()
    .claim(AuthorityClaim {
      scope: CompanionAuthorityScope::NowPlayingMetadata,
      app_bundle: Some("com.spotify.client".to_string()),
    })
    .await
    .expect("claim metadata authority");

  use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  };
  let iap2_done = Arc::new(AtomicBool::new(false));
  let iap2_done_w = iap2_done.clone();
  let iap2_flood = tokio::spawn(async move {
    let mut n: u64 = 0;
    loop {
      let pid = 0x70000 + n;
      let res = iap2
        .push_now_playing(Iap2NowPlaying {
          media_item: Some(Iap2MediaItem {
            persistent_id: Some(pid),
            title: Some(format!("Flood iAP2 {n:05}")),
            artist: Some("Flood Artist".into()),
            ..Default::default()
          }),
          playback: Some(PlaybackAttributes {
            state: Some(PlaybackState::Playing),
            position_ms: Some(((n * 71) % 200_000) as u32),
            ..Default::default()
          }),
        })
        .await;
      if res.is_err() {
        break;
      }
      if n % 100 == 50 {
        let _ = iap2.push_artwork(((n % 240) as u8) + 1, vec![0xAA; 1024]).await;
      }
      n += 1;
      tokio::time::sleep(Duration::from_millis(20)).await;
      if iap2_done_w.load(Ordering::Relaxed) {
        break;
      }
    }
    n
  });

  let gateway_done = Arc::new(AtomicBool::new(false));
  let gateway_done_w = gateway_done.clone();
  let companion_flood = tokio::spawn(async move {
    let mut m: u64 = 0;
    loop {
      let res = gateway
        .player()
        .snapshot(PlayerState {
          track: Some(MediaItem {
            persistent_id: Some(format!("companion:track:{m:05}")),
            title: Some(format!("Flood Companion {m:05}")),
            artist: Some("Companion Artist".into()),
            ..Default::default()
          }),
          playback: Playback {
            state: if m % 2 == 0 {
              libbridgething::PlaybackState::Playing
            } else {
              libbridgething::PlaybackState::Paused
            },
            position_ms: ((m * 137) % 240_000) as u32,
            ..Default::default()
          },
          ..Default::default()
        })
        .await;
      if res.is_err() {
        break;
      }
      if m % 64 == 0 {
        let _ = gateway
          .authority()
          .claim(AuthorityClaim {
            scope: CompanionAuthorityScope::NowPlayingMetadata,
            app_bundle: Some("com.spotify.client".to_string()),
          })
          .await;
      }
      m += 1;
      tokio::time::sleep(Duration::from_millis(15)).await;
      if gateway_done_w.load(Ordering::Relaxed) {
        break;
      }
    }
    m
  });

  let flood_duration = Duration::from_secs(20);
  tokio::time::sleep(flood_duration).await;
  iap2_done.store(true, Ordering::Relaxed);
  gateway_done.store(true, Ordering::Relaxed);

  let iap2_sent = iap2_flood.await.unwrap_or(0);
  let companion_sent = companion_flood.await.unwrap_or(0);
  eprintln!("[flood] sent iAP2={iap2_sent} companion={companion_sent} over {flood_duration:?}");

  let probe_iap2 = device.iap2_source().await.expect("iap2 source for liveness probe");
  let marker = "FloodLivenessProbe-DistinctMarker-XYZ123";
  probe_iap2
    .push_now_playing(Iap2NowPlaying {
      media_item: Some(Iap2MediaItem {
        persistent_id: Some(0xDEADBEEFu64),
        title: Some(marker.into()),
        ..Default::default()
      }),
      playback: None,
    })
    .await
    .expect("probe push");

  let alive = frames
    .wait_for(Duration::from_secs(10), |f| f.json().contains(marker))
    .await;
  assert!(
    alive.is_some(),
    "after {flood_duration:?} of flood (iAP2 deltas={iap2_sent}, companion deltas={companion_sent}), \
     the daemon did NOT broadcast a follow-up marker - field stall reproduced"
  );
}
