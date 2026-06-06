//! Asset family (T1-only): stock `get_image` and modern `asset.get` share one
//! id-keyed resolution path. Covers cache-hit serve, the `iap2/art/...` lane that
//! must never be fetched from the companion, fail-fast when a companion drops an
//! in-flight pull-on-miss request, and the companion-served pull bodies: inline,
//! fragment-streamed, and fragment-streamed under concurrent background traffic.

use std::time::Duration;

use base64::Engine;
use bridgething_gateway::Gateway;
use bridgething_test_harness::{Harness, MockWsClient};
use libbridgething::{
  AssetRetention, GatewayCapabilities, GatewayInfo, Priority,
  gateway::{
    AssetGotReply, BridgeToGatewayAssetMsg, BridgeToGatewayMsgData, GatewayToBridgeAssetMsg, GatewayToBridgeMsgData,
    GatewayToBridgeTransferMsg, TransferBody, TransferFragment, TransferRef,
  },
  wire::MsgMeta,
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

/// Stock `get_image` resolves through the same id-keyed path as the modern
/// `asset.get`, so any cached asset is served as image bytes.
#[tokio::test]
async fn stock_get_image_serves_a_cached_asset() {
  let harness = Harness::start().await.expect("harness start");
  let id = "spotify/track/stockserve/image";
  harness
    .state()
    .assets
    .insert(
      id.into(),
      vec![0x42u8; 128].into(),
      Some("image/jpeg".into()),
      AssetRetention::Lru,
    )
    .await
    .expect("seed cache");

  let mut stock = harness.connect_stock_client().await.expect("stock client");
  let image_data = stock_get_image(&mut stock, id, Duration::from_secs(3))
    .await
    .expect("stock get_image must respond");
  assert!(
    !image_data.is_empty(),
    "stock get_image must serve the cached asset bytes"
  );
}

/// A companion-fetched asset request that is in flight when the companion drops
/// must fail fast, not hang on the request leak-guard. The daemon fails pending
/// gateway requests when their peer disconnects, so the stock reply comes back
/// empty within the disconnect latency rather than after the 60s leak-guard.
#[tokio::test]
async fn companion_disconnect_fails_inflight_asset_request_fast() {
  let harness = Harness::start().await.expect("harness start");
  let companion = harness.connect_android().await.expect("connect companion");
  let mut stock = harness.connect_stock_client().await.expect("stock client");

  // a normal companion-fetched id: cache miss, not an iap2 art id, gateway present
  // -> daemon issues an AssetRequest the companion never answers.
  let id = "spotify/img/480/https%3A%2F%2Fexample.test%2Fart.jpg";

  let drop_companion = async {
    tokio::time::sleep(Duration::from_millis(300)).await;
    drop(companion);
  };
  let fetch = stock_get_image(&mut stock, id, Duration::from_secs(8));
  let (image, ()) = tokio::join!(fetch, drop_companion);

  assert_eq!(
    image,
    Some(String::new()),
    "a disconnect must fail the in-flight fetch promptly and serve an empty image, not hang"
  );
}

/// Announce companion capabilities so the daemon opens the companion-fetch
/// lane for cache misses.
async fn announce(gateway: &Gateway) {
  let caps = GatewayCapabilities {
    gateway: GatewayInfo {
      address: String::new(),
      name: "asset-server".into(),
      os_name: "android".into(),
      app_name: "asset-server".into(),
      app_version: "0.0.0".into(),
      adapter_version: "harness".into(),
      lib_version: "0.0.0".into(),
      libbridgething_version: format!("v{}", libbridgething::LIBBRIDGETHING_VERSION),
    },
    ..Default::default()
  };
  gateway.capabilities().announce(caps).await.expect("announce");
}

/// Answer every daemon `AssetRequest` with `body`: inline bytes, or a
/// fragment stream on the Bulk lane keyed by the request id.
fn serve_assets(gateway: Gateway, bytes: Vec<u8>, inline: bool, fragment_size: usize) -> tokio::task::JoinHandle<()> {
  let mut events = gateway.events();
  tokio::spawn(async move {
    while let Ok(msg) = events.recv().await {
      let BridgeToGatewayMsgData::Asset(BridgeToGatewayAssetMsg::Request(req)) = &msg.data else {
        continue;
      };
      let req = req.clone();
      let body = if inline {
        TransferBody::Inline(bytes.clone())
      } else {
        TransferBody::Stream(TransferRef {
          id: req.request_id,
          total_size: bytes.len() as u32,
          sha256: None,
        })
      };
      let reply = GatewayToBridgeMsgData::Asset(GatewayToBridgeAssetMsg::Got(AssetGotReply {
        id: req.id.clone(),
        mime: Some("image/jpeg".into()),
        body,
      }));
      gateway.connection().respond(msg.id, reply).await.expect("respond");
      if !inline {
        let mut offset = 0usize;
        while offset < bytes.len() {
          let end = (offset + fragment_size).min(bytes.len());
          gateway
            .connection()
            .send_data(
              MsgMeta::Event,
              GatewayToBridgeMsgData::Transfer(GatewayToBridgeTransferMsg::Fragment(TransferFragment {
                transfer_id: req.request_id,
                offset: offset as u32,
                bytes: bytes[offset..end].to_vec(),
              })),
              Priority::Bulk,
            )
            .await
            .expect("fragment send");
          offset = end;
        }
      }
    }
  })
}

fn art_bytes(len: usize) -> Vec<u8> {
  (0u8..=255).cycle().take(len).collect()
}

/// Companion serves an asset as a Bulk-lane fragment stream; the daemon
/// reassembles it through the memory sink and serves the exact bytes.
#[tokio::test]
async fn asset_pull_stream_body_serves_exact_bytes() {
  let harness = Harness::start().await.expect("harness start");
  let companion = harness.connect_android().await.expect("connect companion");
  announce(&companion).await;
  let bytes = art_bytes(100 * 1024);
  let _responder = serve_assets(companion.clone(), bytes.clone(), false, 8 * 1024);

  let mut stock = harness.connect_stock_client().await.expect("stock client");
  let image_data = stock_get_image(&mut stock, "spotify/img/248/streamed", ASSET_RESOLVE)
    .await
    .expect("stock get_image must respond");
  assert_eq!(
    image_data,
    base64::engine::general_purpose::STANDARD.encode(&bytes),
    "served bytes must match the fragment-streamed source"
  );
}

/// Companion serves a small asset inline in the reply; no fragments flow.
#[tokio::test]
async fn asset_pull_inline_body_serves_exact_bytes() {
  let harness = Harness::start().await.expect("harness start");
  let companion = harness.connect_android().await.expect("connect companion");
  announce(&companion).await;
  let bytes = art_bytes(4 * 1024);
  let _responder = serve_assets(companion.clone(), bytes.clone(), true, 0);

  let mut stock = harness.connect_stock_client().await.expect("stock client");
  let image_data = stock_get_image(&mut stock, "spotify/img/96/inline", ASSET_RESOLVE)
    .await
    .expect("stock get_image must respond");
  assert_eq!(image_data, base64::engine::general_purpose::STANDARD.encode(&bytes));
}

/// A sustained Background-lane fragment flood (a stand-in for an OTA push)
/// must not stall a foreground asset pull: the companion's lane scheduler
/// drains the Bulk-lane art fragments ahead of the queued Background
/// backlog, and the daemon drops the unknown-id flood without harm.
#[tokio::test]
async fn asset_pull_resolves_under_background_flood() {
  let harness = Harness::start().await.expect("harness start");
  let companion = harness.connect_android().await.expect("connect companion");
  announce(&companion).await;
  let bytes = art_bytes(64 * 1024);
  let _responder = serve_assets(companion.clone(), bytes.clone(), false, 8 * 1024);

  let flood_id = uuid::Uuid::now_v7();
  let flooder = {
    let companion = companion.clone();
    tokio::spawn(async move {
      let chunk = vec![0xAAu8; 8 * 1024];
      for i in 0u32..512 {
        if companion
          .connection()
          .send_data(
            MsgMeta::Event,
            GatewayToBridgeMsgData::Transfer(GatewayToBridgeTransferMsg::Fragment(TransferFragment {
              transfer_id: flood_id,
              offset: i * 8 * 1024,
              bytes: chunk.clone(),
            })),
            Priority::Background,
          )
          .await
          .is_err()
        {
          break;
        }
      }
    })
  };

  // let the flood get ahead before asking for art.
  tokio::time::sleep(Duration::from_millis(50)).await;

  let mut stock = harness.connect_stock_client().await.expect("stock client");
  let image_data = stock_get_image(&mut stock, "spotify/img/248/contended", ASSET_RESOLVE)
    .await
    .expect("asset pull must resolve while a background flood is in flight");
  assert_eq!(
    image_data,
    base64::engine::general_purpose::STANDARD.encode(&bytes),
    "served bytes must be intact under background contention"
  );
  flooder.abort();
}
