//! Tier-2: the daemon's cdp overlay injection against a real headless chromium.
//! The daemon's chrome worker dials a fixed debug port (BRIDGETHING_CHROME_PORT),
//! installs the embedded overlay bundle via addScriptToEvaluateOnNewDocument, and
//! the served page mounts the shadow host. One test fn: the env var is process
//! global, and this file is its own test binary. Skips (does not fail) when no
//! chromium binary is present.

use std::time::Duration;

use bridgething_test_harness::Harness;
use chromiumoxide::{Browser, BrowserConfig, Page};
use futures::StreamExt;
use uuid::Uuid;

const HOST_PROBE: &str = "!!document.querySelector('bridgething-overlay')";
const HOST_COUNT: &str = "document.querySelectorAll('bridgething-overlay').length";

fn free_port() -> u16 {
  std::net::TcpListener::bind("127.0.0.1:0")
    .expect("free port probe")
    .local_addr()
    .expect("probe addr")
    .port()
}

async fn plant(harness: &Harness, id: Uuid, overlays: Option<&str>) {
  let dir = harness.state_dir().join("webapps").join(id.simple().to_string());
  std::fs::create_dir_all(&dir).expect("bundle dir");
  std::fs::write(dir.join("index.html"), b"<html><body><h1>app</h1></body></html>").expect("index");
  let overlays_field = overlays.map(|o| format!(r#","overlays":{o}"#)).unwrap_or_default();
  let manifest = format!(r#"{{"id":"{id}","name":"planted","version":"0.1.0"{overlays_field}}}"#);
  std::fs::write(dir.join("manifest.json"), manifest).expect("manifest");
  harness.state().webapps.rescan().await;
}

async fn overlay_mounted(page: &Page) -> bool {
  matches!(
    page.evaluate(HOST_PROBE).await.map(|r| r.into_value::<bool>()),
    Ok(Ok(true))
  )
}

/// Navigate fresh and report whether the overlay host mounted in that document,
/// retrying to absorb the daemon worker's async install of the cdp script.
async fn settle(page: &Page, url: &str, want_mounted: bool) -> bool {
  for _ in 0..40 {
    if page.goto(url).await.is_err() {
      tokio::time::sleep(Duration::from_millis(250)).await;
      continue;
    }
    let _ = page.wait_for_navigation().await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    if overlay_mounted(page).await == want_mounted {
      return true;
    }
    tokio::time::sleep(Duration::from_millis(250)).await;
  }
  false
}

#[tokio::test]
async fn t2_overlay_injection_follows_the_active_manifest() {
  let port = free_port();
  // safety: single-threaded at this point, and this file is its own test process
  unsafe { std::env::set_var("BRIDGETHING_CHROME_PORT", port.to_string()) };

  let config = match BrowserConfig::builder()
    .no_sandbox()
    .window_size(800, 480)
    .port(port)
    .arg("--disable-dev-shm-usage")
    .build()
  {
    Ok(c) => c,
    Err(err) => {
      eprintln!("skipping overlay T2: chromium config unavailable ({err})");
      return;
    }
  };
  let (browser, mut events) = match Browser::launch(config).await {
    Ok(pair) => pair,
    Err(err) => {
      eprintln!("skipping overlay T2: chromium unavailable ({err})");
      return;
    }
  };
  let handler = tokio::spawn(async move {
    while let Some(event) = events.next().await {
      if event.is_err() {
        break;
      }
    }
  });

  let page = match browser.pages().await.ok().and_then(|mut p| p.pop()) {
    Some(p) => p,
    None => browser.new_page("about:blank").await.expect("initial page"),
  };

  let harness = Harness::start().await.expect("harness start");
  let url = format!("http://{}/", harness.modern_addr());

  let on_id = Uuid::now_v7();
  let off_id = Uuid::now_v7();
  plant(&harness, on_id, None).await;
  plant(
    &harness,
    off_id,
    Some(r#"{"notifications":false,"call":false,"pairing":false,"connection":false,"volume":false}"#),
  )
  .await;

  // an undeclared manifest defaults every surface on: the overlay mounts
  harness.state().set_active_webapp(on_id).await.expect("activate on-app");
  assert!(
    settle(&page, &url, true).await,
    "overlay host never mounted for a default-manifest webapp"
  );

  // daemon-restart bootstrap evaluates into the live document; the mount guard holds
  harness.state().sync_overlay(true).await;
  tokio::time::sleep(Duration::from_secs(1)).await;
  let count: i64 = page
    .evaluate(HOST_COUNT)
    .await
    .expect("count eval")
    .into_value()
    .expect("count value");
  assert_eq!(count, 1, "run_immediately re-install must not double-mount");

  // an all-off manifest removes the injected script for the next document
  harness
    .state()
    .set_active_webapp(off_id)
    .await
    .expect("activate off-app");
  assert!(
    settle(&page, &url, false).await,
    "overlay host still mounts for an all-off manifest"
  );

  handler.abort();
}
