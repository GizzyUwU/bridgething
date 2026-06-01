//! Asset family (T1-only): the chunked-transfer disk tier surviving a daemon
//! restart. Headless uses an in-memory db but on-disk blob + transfer dirs, so
//! sqlite metadata does not survive a restart but a chunked push's `.partial` +
//! `.meta` sidecar do - which is exactly the resume path worth exercising. This
//! takes ownership of the harness (restart consumes it), so it is a direct T1
//! test rather than a lifted seam body; daemon restart is a T1 concept anyway.

use std::time::Duration;

use bridgething_test_harness::{Harness, MockWsClient};
use libbridgething::{
  AssetRetention,
  client::AssetGet,
  gateway::{AssetPush, AssetPushBegin, AssetPushChunk},
};

const ASSET_RESOLVE: Duration = Duration::from_secs(5);

async fn stock_get_image(stock: &mut MockWsClient, id: &str, timeout: Duration) -> Option<String> {
  let req = format!(r#"{{"msgId":4242,"method":"com.spotify.get_image","args":{{"id":"{id}"}},"userAction":false}}"#);
  stock.send_text(req).await.expect("send get_image");
  let deadline = tokio::time::Instant::now() + timeout;
  loop {
    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
    if remaining.is_zero() {
      return None;
    }
    let text = match tokio::time::timeout(remaining, stock.recv()).await {
      Ok(Some(t)) => t,
      _ => return None,
    };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) else {
      continue;
    };
    if val.get("msgId").and_then(serde_json::Value::as_u64) != Some(4242) {
      continue;
    }
    return Some(
      val
        .get("payload")
        .and_then(|p| p.get("image_data"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string(),
    );
  }
}

/// Stock and modern asset requests share one resolution path, keyed by id. An
/// `iap2/art/...` id must take the iAP2-wait lane and never be fetched from the
/// companion (iOS pushes art; it cannot serve it on request). A companion IS
/// connected here so the companion-fetch lane is live - routing the iap2 id there
/// would await a response no one sends, hanging the stock reply.
#[tokio::test]
async fn stock_get_image_never_routes_iap2_art_to_the_companion() {
  let harness = Harness::start().await.expect("harness start");
  let _companion = harness.connect_android().await.expect("connect companion");
  let mut stock = harness.connect_stock_client().await.expect("stock client");

  let image_data = stock_get_image(&mut stock, "iap2/art/deadbeef/9", Duration::from_secs(3))
    .await
    .expect("stock get_image for an iap2 art id must respond promptly, not hang");
  assert!(
    image_data.is_empty(),
    "a non-pending iap2 art id must resolve empty, never be fetched from the companion"
  );
}

/// Stock `get_image` resolves through the same path as the modern `asset.get`,
/// so a cached asset (here companion-pushed) is served as image bytes.
#[tokio::test]
async fn stock_get_image_serves_a_companion_cached_asset() {
  let harness = Harness::start().await.expect("harness start");
  let gateway = harness.connect_android().await.expect("connect companion");
  let id = "spotify/track/stockserve/image";
  gateway
    .asset()
    .push(AssetPush {
      id: id.into(),
      bytes: vec![0x42u8; 128],
      mime: Some("image/jpeg".into()),
      retention: AssetRetention::Lru,
    })
    .await
    .expect("companion push");

  let deadline = tokio::time::Instant::now() + ASSET_RESOLVE;
  while harness.state().assets.get(id).await.ok().flatten().is_none() {
    assert!(tokio::time::Instant::now() < deadline, "pushed asset never cached");
    tokio::time::sleep(Duration::from_millis(25)).await;
  }

  let mut stock = harness.connect_stock_client().await.expect("stock client");
  let image_data = stock_get_image(&mut stock, id, Duration::from_secs(3))
    .await
    .expect("stock get_image must respond");
  assert!(!image_data.is_empty(), "stock get_image must serve the cached asset bytes");
}

#[tokio::test]
async fn chunked_asset_push_resumes_across_daemon_restart() {
  let harness = Harness::start().await.expect("harness start");
  let gateway = harness.connect_android().await.expect("connect companion");

  let id = "spotify/track/resume-across-restart/image".to_string();
  let payload: Vec<u8> = (0..4096u32).map(|i| (i % 251) as u8).collect();
  let half = payload.len() / 2;
  let begin = || AssetPushBegin {
    id: id.clone(),
    expected_size: payload.len() as u32,
    expected_sha256: None,
    mime: Some("image/jpeg".into()),
    retention: AssetRetention::Persistent,
  };

  let ack = gateway.asset().push_begin(begin()).await.expect("push_begin");
  assert_eq!(ack.resume_from_offset, 0, "a fresh push must start at offset 0");

  gateway
    .asset()
    .push_chunk(AssetPushChunk {
      id: id.clone(),
      offset: 0,
      bytes: payload[..half].to_vec(),
      last: false,
    })
    .await
    .expect("push first half");

  // restart: in-memory db resets, on-disk transfers/ persists.
  let harness = harness.restart().await.expect("restart");
  let gateway = harness.connect_android().await.expect("reconnect companion");

  let ack = gateway
    .asset()
    .push_begin(begin())
    .await
    .expect("push_begin after restart");
  assert!(
    ack.resume_from_offset > 0,
    "the persisted partial must survive restart, but push_begin reported resume_from_offset=0"
  );
  let resume = ack.resume_from_offset as usize;

  gateway
    .asset()
    .push_chunk(AssetPushChunk {
      id: id.clone(),
      offset: ack.resume_from_offset,
      bytes: payload[resume..].to_vec(),
      last: true,
    })
    .await
    .expect("push remaining after resume");

  // the reassembled asset must resolve with the exact original bytes.
  let client = harness.connect_command_client().await.expect("command client");
  let deadline = tokio::time::Instant::now() + ASSET_RESOLVE;
  let got = loop {
    if let Ok(got) = client
      .asset()
      .get(AssetGet {
        id: id.clone(),
        request_id: uuid::Uuid::now_v7(),
      })
      .await
    {
      break got;
    }
    assert!(
      tokio::time::Instant::now() < deadline,
      "asset never resolved after the resumed push completed"
    );
    tokio::time::sleep(Duration::from_millis(25)).await;
  };
  assert_eq!(got.bytes, payload, "resumed asset bytes must match the original");
}
