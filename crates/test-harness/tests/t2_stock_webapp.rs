use std::time::Duration;

use bridgething_test_harness::Harness;

#[tokio::test]
async fn t2_stock_spa_connects_and_renders() {
  let harness = match Harness::start_with_stock_webapp().await {
    Ok(h) => h,
    Err(err) => {
      eprintln!("skipping T2 smoke: stock bundle unavailable ({err})");
      return;
    }
  };

  let chrome = match harness.open_stock_chrome().await {
    Ok(c) => c,
    Err(err) => {
      eprintln!("skipping T2 smoke: chromium unavailable ({err})");
      return;
    }
  };

  let loaded = chrome
    .wait_for_js("document.readyState === 'complete'", Duration::from_secs(15))
    .await;
  assert!(loaded, "stock SPA document never reached readyState complete");

  let connected = harness
    .wait_for(|state| state.client_man.client_count() >= 1, Duration::from_secs(15))
    .await;
  assert!(connected, "stock SPA never established its websocket to the daemon");
}
