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
    AssetGotReply, AssetNotFoundReply, AssetRequest, BridgeToGatewayAssetMsg, BridgeToGatewayMsgData,
    GatewayToBridgeAssetMsg, GatewayToBridgeMsgData, GatewayToBridgeTransferMsg, TransferBody, TransferFragment,
    TransferRef,
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

#[derive(Debug, PartialEq)]
enum StockImageReply {
  Bytes,
  EmptySuccess,
  Error,
}

async fn stock_image_reply(stock: &mut MockWsClient, id: &str, msg_id: u64, timeout: Duration) -> StockImageReply {
  let req =
    format!(r#"{{"msgId":{msg_id},"method":"com.spotify.get_image","args":{{"id":"{id}"}},"userAction":false}}"#);
  stock.send_text(req).await.expect("send get_image");
  let deadline = tokio::time::Instant::now() + timeout;
  loop {
    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
    assert!(!remaining.is_zero(), "stock image request {msg_id} never got a reply");
    let text = match tokio::time::timeout(remaining, stock.recv()).await {
      Ok(Some(t)) => t,
      Ok(None) => panic!("stock socket closed awaiting reply to {msg_id}"),
      Err(_) => panic!("stock image request {msg_id} timed out with no reply"),
    };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) else {
      continue;
    };
    if val.get("msgId").and_then(serde_json::Value::as_u64) != Some(msg_id) {
      continue;
    }
    if val.get("type").and_then(serde_json::Value::as_str) == Some("call_error") {
      return StockImageReply::Error;
    }
    let data = val
      .get("payload")
      .and_then(|p| p.get("image_data"))
      .and_then(serde_json::Value::as_str)
      .unwrap_or("");
    return if data.is_empty() {
      StockImageReply::EmptySuccess
    } else {
      StockImageReply::Bytes
    };
  }
}

fn serve_asset_not_found(gateway: Gateway) -> tokio::task::JoinHandle<()> {
  let mut events = gateway.events();
  tokio::spawn(async move {
    while let Ok(msg) = events.recv().await {
      let BridgeToGatewayMsgData::Asset(BridgeToGatewayAssetMsg::Request(req)) = &msg.data else {
        continue;
      };
      gateway
        .connection()
        .respond_err::<AssetRequest>(msg.id, AssetNotFoundReply { id: req.id.clone() })
        .await
        .expect("respond not found");
    }
  })
}

#[tokio::test]
async fn stock_image_miss_settles_instead_of_spinning() {
  let harness = Harness::start().await.expect("harness start");
  let companion = harness.connect_android().await.expect("connect companion");
  announce(&companion).await;
  let _responder = serve_asset_not_found(companion.clone());
  let mut stock = harness.connect_stock_client().await.expect("stock client");

  let id = "spotify/img/248/iflickerrepro";
  const REFETCH_CAP: u64 = 40;
  let per_request = Duration::from_secs(40);

  let mut requests = 0u64;
  for msg_id in 1..=REFETCH_CAP {
    requests += 1;
    if stock_image_reply(&mut stock, id, msg_id, per_request).await != StockImageReply::EmptySuccess {
      break;
    }
  }

  assert!(
    requests < REFETCH_CAP,
    "stock refetched {requests} times without settling; an empty success re-arms the fetch forever"
  );
}

#[tokio::test]
async fn stock_get_image_never_routes_iap2_art_to_the_companion() {
  let harness = Harness::start().await.expect("harness start");
  let _companion = harness.connect_android().await.expect("connect companion");
  let mut stock = harness.connect_stock_client().await.expect("stock client");

  let reply = stock_image_reply(&mut stock, "iap2/art/deadbeef/9", 7001, Duration::from_secs(3)).await;
  assert_eq!(
    reply,
    StockImageReply::Error,
    "a non-pending iap2 art id is authoritatively absent: fail it promptly rather than fetching it \
     from the companion or answering empty, which would re-arm stock's fetch"
  );
}

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

#[tokio::test]
async fn companion_disconnect_fails_inflight_asset_request_fast() {
  let harness = Harness::start().await.expect("harness start");
  let companion = harness.connect_android().await.expect("connect companion");
  let mut stock = harness.connect_stock_client().await.expect("stock client");

  let id = "spotify/img/480/https%3A%2F%2Fexample.test%2Fart.jpg";

  let drop_companion = async {
    tokio::time::sleep(Duration::from_millis(300)).await;
    drop(companion);
  };
  let fetch = stock_image_reply(&mut stock, id, 7002, Duration::from_secs(8));
  let (reply, ()) = tokio::join!(fetch, drop_companion);

  assert_eq!(
    reply,
    StockImageReply::Error,
    "a disconnect closes the only lane that could produce bytes, so the fetch must fail promptly \
     rather than hang on the leak-guard or answer empty"
  );
}

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
