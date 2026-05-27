//! Asset family (T1-only): the chunked-transfer disk tier surviving a daemon
//! restart. Headless uses an in-memory db but on-disk blob + transfer dirs, so
//! sqlite metadata does not survive a restart but a chunked push's `.partial` +
//! `.meta` sidecar do - which is exactly the resume path worth exercising. This
//! takes ownership of the harness (restart consumes it), so it is a direct T1
//! test rather than a lifted seam body; daemon restart is a T1 concept anyway.

use std::time::Duration;

use bridgething_test_harness::Harness;
use libbridgething::{
  AssetRetention,
  client::AssetGet,
  gateway::{AssetPushBegin, AssetPushChunk},
};

const ASSET_RESOLVE: Duration = Duration::from_secs(5);

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
