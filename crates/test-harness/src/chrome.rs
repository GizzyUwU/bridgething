//! Tier-2 observer: drive the real stock SPA under headless chromium via CDP.
//! chromiumoxide launches a host chromium; the daemon serves the real bundle
//! off its modern http port and the SPA's own javascript runs untouched. The
//! SPA hardcodes its websocket as ws://localhost:8890, but headless binds
//! ephemeral ports, so a pre-load shim rewrites that port to the daemon's bound
//! stock port - every daemon keeps its own ports so parallel tiers never collide.

use std::{net::SocketAddr, time::Duration};

use anyhow::{Result, anyhow};
use chromiumoxide::{
  Browser, BrowserConfig, Page, cdp::browser_protocol::page::AddScriptToEvaluateOnNewDocumentParams,
};
use futures::StreamExt;
use serde::de::DeserializeOwned;
use tokio::task::JoinHandle;

/// A headless chromium attached to the daemon, rendering the real stock SPA.
/// Drop kills the browser (chromiumoxide sets kill-on-drop) and ends the driver.
pub struct ChromeView {
  page: Page,
  _browser: Browser,
  handler: JoinHandle<()>,
}

impl ChromeView {
  /// Launch chromium, install the websocket-port shim before any page script
  /// runs, then navigate to the daemon's modern http port (which serves the
  /// active webapp - the stock SPA). Errors when no chromium binary is present,
  /// which lets callers skip rather than fail on a runner without one.
  pub async fn launch(modern_addr: SocketAddr, stock_port: u16) -> Result<Self> {
    let config = BrowserConfig::builder()
      .no_sandbox()
      .window_size(800, 480)
      .build()
      .map_err(|err| anyhow!("chromium config: {err}"))?;
    let (browser, mut events) = Browser::launch(config).await?;
    let handler = tokio::spawn(async move {
      while let Some(event) = events.next().await {
        if event.is_err() {
          break;
        }
      }
    });

    let page = browser.new_page("about:blank").await?;
    page
      .execute(AddScriptToEvaluateOnNewDocumentParams::new(ws_port_shim(stock_port)))
      .await?;
    page.goto(format!("http://{modern_addr}/")).await?;
    page.wait_for_navigation().await?;

    Ok(Self {
      page,
      _browser: browser,
      handler,
    })
  }

  /// Evaluate javascript in the page and deserialize the result.
  pub async fn eval<T: DeserializeOwned>(&self, js: impl Into<String>) -> Result<T> {
    Ok(self.page.evaluate(js.into()).await?.into_value()?)
  }

  /// Poll a boolean javascript expression until it holds or the timeout elapses.
  /// The daemon applies updates asynchronously and the SPA renders
  /// asynchronously, so dom assertions converge rather than being instantaneous.
  pub async fn wait_for_js(&self, expr: &str, timeout: Duration) -> bool {
    let probe = format!("(() => {{ try {{ return !!({expr}); }} catch (e) {{ return false; }} }})()");
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
      if let Ok(true) = self.eval::<bool>(probe.clone()).await {
        return true;
      }
      if tokio::time::Instant::now() >= deadline {
        return false;
      }
      tokio::time::sleep(Duration::from_millis(50)).await;
    }
  }
}

impl Drop for ChromeView {
  fn drop(&mut self) {
    self.handler.abort();
  }
}

fn ws_port_shim(stock_port: u16) -> String {
  format!(
    r#"(function () {{
  var Orig = window.WebSocket;
  function Patched(url, protocols) {{
    try {{
      var u = new URL(url);
      if (u.port === '8890') {{ u.hostname = '127.0.0.1'; u.port = '{stock_port}'; url = u.toString(); }}
    }} catch (e) {{}}
    return protocols === undefined ? new Orig(url) : new Orig(url, protocols);
  }}
  Patched.prototype = Orig.prototype;
  Patched.CONNECTING = Orig.CONNECTING;
  Patched.OPEN = Orig.OPEN;
  Patched.CLOSING = Orig.CLOSING;
  Patched.CLOSED = Orig.CLOSED;
  window.WebSocket = Patched;
}})();"#
  )
}
