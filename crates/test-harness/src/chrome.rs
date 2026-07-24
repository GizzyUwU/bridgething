use std::{net::SocketAddr, time::Duration};

use anyhow::{Result, anyhow};
use chromiumoxide::{
  Browser, BrowserConfig, Page, cdp::browser_protocol::page::AddScriptToEvaluateOnNewDocumentParams,
};
use futures::StreamExt;
use serde::de::DeserializeOwned;
use tokio::task::JoinHandle;

pub struct ChromeView {
  page: Page,
  _browser: Browser,
  handler: JoinHandle<()>,
}

impl ChromeView {
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

  pub async fn eval<T: DeserializeOwned>(&self, js: impl Into<String>) -> Result<T> {
    Ok(self.page.evaluate(js.into()).await?.into_value()?)
  }

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
