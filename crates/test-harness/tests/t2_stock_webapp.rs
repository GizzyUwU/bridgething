//! Tier-2 smoke: the *real* stock SPA, served by the headless daemon and run
//! under headless chromium, connects its websocket back to the daemon. Proves
//! the whole T2 rig end to end - bundle served on the modern http port, the
//! SPA's own javascript runs, the ws-port shim retargets the ephemeral stock
//! port, the daemon accepts the stock client - and that CDP can observe the dom.
//! Skips (does not fail) when the stock dist or a chromium binary is absent, so
//! it is safe on a runner without either.

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

  // CDP can observe the dom: the served document finished loading.
  let loaded = chrome
    .wait_for_js("document.readyState === 'complete'", Duration::from_secs(15))
    .await;
  assert!(loaded, "stock SPA document never reached readyState complete");

  // the SPA's own javascript opened its websocket (shim-retargeted to the
  // ephemeral stock port) and the daemon registered it as a stock client - the
  // full serve + run + connect chain, not a hand-driven mock.
  let connected = harness
    .wait_for(|state| state.client_man.client_count() >= 1, Duration::from_secs(15))
    .await;
  assert!(connected, "stock SPA never established its websocket to the daemon");
}
