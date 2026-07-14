use std::{
  sync::{Arc, atomic::AtomicBool},
  time::Duration,
};

use headless_chrome::{
  Browser, Tab,
  protocol::cdp::{
    Network,
    Page::{
      AddScriptToEvaluateOnNewDocument, GetNavigationHistory, NavigateToHistoryEntry,
      RemoveScriptToEvaluateOnNewDocument,
    },
  },
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

const ENV_CHROME_PORT: &str = "BRIDGETHING_CHROME_PORT";

const CHROME_IDLE_TIMEOUT: Duration = Duration::from_secs(60 * 60 * 24 * 365 * 10);
const CHROME_PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const CHROME_CONNECT_BACKOFF: Duration = Duration::from_secs(30);

#[cfg(not(debug_assertions))]
const DEFAULT_CHROME_PORT: u16 = 9223;
#[cfg(debug_assertions)]
const DEFAULT_CHROME_PORT: u16 = 9222;

fn chrome_port() -> u16 {
  if let Ok(raw) = std::env::var(ENV_CHROME_PORT) {
    match raw.parse::<u16>() {
      Ok(p) => return p,
      Err(_) => {
        tracing::warn!("ignoring invalid {ENV_CHROME_PORT}={raw:?}, falling back to default {DEFAULT_CHROME_PORT}")
      }
    }
  }
  DEFAULT_CHROME_PORT
}

fn chrome_status_url() -> String {
  format!("http://127.0.0.1:{}/json/version", chrome_port())
}

type ChromeTx = tokio::sync::mpsc::Sender<ChromeCommand>;
type ChromeRx = tokio::sync::mpsc::Receiver<ChromeCommand>;

#[derive(Debug, Clone)]
pub enum ChromeCommand {
  Navigate(String),
  NavigateExternal(String),
  HistoryBack,
  HistoryForward,
  Reload,
  ClearHttpCache,
  SetOverlay {
    script: Option<OverlayScript>,
    run_immediately: bool,
  },
}

#[derive(Clone)]
pub struct OverlayScript(pub Arc<String>);

impl std::fmt::Debug for OverlayScript {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "OverlayScript({} bytes)", self.0.len())
  }
}

#[derive(Debug)]
pub struct Chrome {
  connected: Arc<AtomicBool>,
  external: Arc<AtomicBool>,
  tx: ChromeTx,

  cancel_token: tokio_util::sync::CancellationToken,
  _worker: tokio::task::JoinHandle<()>,
}

impl Chrome {
  pub async fn init() -> Result<Self> {
    tracing::debug!("initializing chrome worker (port {})", chrome_port());
    let (tx, rx) = tokio::sync::mpsc::channel(16);
    let connected = Arc::new(AtomicBool::new(false));
    let external = Arc::new(AtomicBool::new(false));

    let cancel_token = tokio_util::sync::CancellationToken::new();
    let mut worker = ChromeWorker::new(connected.clone(), external.clone(), rx, cancel_token.clone())?;

    Ok(Self {
      connected: connected.clone(),
      external,
      tx,

      cancel_token: cancel_token.clone(),
      _worker: tokio::spawn(async move { worker.run().await }),
    })
  }

  pub fn connected(&self) -> bool {
    self.connected.load(std::sync::atomic::Ordering::SeqCst)
  }

  pub fn is_external(&self) -> bool {
    self.external.load(std::sync::atomic::Ordering::SeqCst)
  }

  pub async fn send(&self, command: ChromeCommand) -> Result<()> {
    tracing::debug!("sending command to chrome: {:?}", command);
    Ok(self.tx.send(command).await?)
  }

  pub async fn shutdown(&self) {
    self.cancel_token.cancel();
  }
}

struct ChromeWorker {
  connected: Arc<AtomicBool>,
  external: Arc<AtomicBool>,
  browser: Option<Browser>,
  http: reqwest::Client,
  overlay_id: Option<String>,

  rx: ChromeRx,
  cancel_token: CancellationToken,
}

impl ChromeWorker {
  fn new(
    connected: Arc<AtomicBool>,
    external: Arc<AtomicBool>,
    rx: ChromeRx,
    cancel_token: CancellationToken,
  ) -> Result<Self> {
    let http = reqwest::Client::builder()
      .connect_timeout(CHROME_PROBE_TIMEOUT)
      .timeout(CHROME_PROBE_TIMEOUT)
      .build()
      .map_err(|e| ChromeError::Connect(Box::new(e)))?;

    Ok(Self {
      connected,
      external,
      browser: None,
      http,
      overlay_id: None,

      rx,
      cancel_token,
    })
  }

  async fn run(&mut self) {
    loop {
      tokio::select! {
        Some(message) = self.rx.recv() => {
          match message {
            ChromeCommand::Navigate(url) => {
              self.external.store(false, std::sync::atomic::Ordering::SeqCst);
              self.handle_navigate(url).await
            }
            ChromeCommand::NavigateExternal(url) => {
              self.external.store(true, std::sync::atomic::Ordering::SeqCst);
              self.handle_navigate(url).await
            }
            ChromeCommand::HistoryBack => self.handle_history(false).await,
            ChromeCommand::HistoryForward => self.handle_history(true).await,
            ChromeCommand::Reload => {
              self.external.store(false, std::sync::atomic::Ordering::SeqCst);
              self.handle_reload().await
            }
            ChromeCommand::ClearHttpCache => self.handle_clear_http_cache().await,
            ChromeCommand::SetOverlay { script, run_immediately } => {
              self.handle_set_overlay(script, run_immediately).await
            }
          }
        }
        _ = self.cancel_token.cancelled() => {
          tracing::debug!("chrome worker shutting down");
          break;
        }
      }
    }
  }

  async fn handle_reload(&mut self) {
    tracing::debug!("reloading current chrome tab");
    self
      .with_first_tab("reload", |tab| tab.reload(true, None).map(|_| ()))
      .await;
  }

  async fn handle_navigate(&mut self, url: String) {
    tracing::debug!("navigating to {}", url);
    self
      .with_first_tab("navigate", |tab| {
        if url_matches_current(&tab.get_url(), &url) {
          tab.reload(true, None).map(|_| ())
        } else {
          tab.navigate_to(&url).map(|_| ())
        }
      })
      .await;
  }

  async fn handle_history(&mut self, forward: bool) {
    tracing::debug!("history navigate (forward={})", forward);
    self
      .with_first_tab("history", move |tab| {
        let history = tab.call_method(GetNavigationHistory(None))?;
        let current = history.current_index as usize;
        let target = if forward {
          current + 1
        } else if current == 0 {
          return Ok(());
        } else {
          current - 1
        };
        let Some(entry) = history.entries.get(target) else {
          return Ok(());
        };
        let entry_id = entry.id;
        tab.call_method(NavigateToHistoryEntry { entry_id }).map(|_| ())
      })
      .await;
  }

  async fn handle_set_overlay(&mut self, script: Option<OverlayScript>, run_immediately: bool) {
    tracing::debug!(
      installed = script.is_some(),
      run_immediately,
      "setting overlay injection"
    );
    let prior = self.overlay_id.take();
    let new_id = std::sync::Mutex::new(None);
    self
      .with_first_tab("set-overlay", |tab| {
        if let Some(id) = &prior {
          let _ = tab.call_method(RemoveScriptToEvaluateOnNewDocument { identifier: id.clone() });
        }
        if let Some(src) = &script {
          let installed = tab.call_method(AddScriptToEvaluateOnNewDocument {
            source: (*src.0).clone(),
            world_name: None,
            include_command_line_api: None,
            run_immediately: run_immediately.then_some(true),
          })?;
          *new_id.lock().unwrap() = Some(installed.identifier);
        }
        Ok(())
      })
      .await;
    self.overlay_id = new_id.into_inner().unwrap();
  }

  async fn handle_clear_http_cache(&mut self) {
    tracing::debug!("clearing chromium http cache");
    self
      .with_first_tab("clear-http-cache", |tab| {
        tab.call_method(Network::ClearBrowserCache(None)).map(|_| ())
      })
      .await;
  }

  async fn with_first_tab<F>(&mut self, label: &'static str, op: F)
  where
    F: Fn(&Arc<Tab>) -> anyhow::Result<()>,
  {
    for attempt in 0..2u8 {
      let tab = match self.first_tab().await {
        Some(tab) => tab,
        None => {
          // first_tab() already logged + slept on connect failure
          return;
        }
      };

      match safe_call(label, || op(&tab)) {
        Ok(()) => return,
        Err(e) => {
          tracing::warn!("chrome {label} failed (attempt {attempt}): {e:?}; dropping connection");
          self.browser = None;
          self.connected.store(false, std::sync::atomic::Ordering::SeqCst);
        }
      }
    }
    tracing::error!("chrome {label} gave up after one retry");
  }

  async fn first_tab(&mut self) -> Option<Arc<Tab>> {
    if self.browser.is_none() {
      self.connect_browser().await;
    }
    let browser = self.browser.take()?;

    if let Err(e) = catch_panic("register_missing_tabs", || browser.register_missing_tabs()) {
      tracing::warn!("{e:?}; dropping cached browser");
      self.connected.store(false, std::sync::atomic::Ordering::SeqCst);
      return None;
    }

    let tab = {
      let guard = match browser.get_tabs().lock() {
        Ok(g) => g,
        Err(e) => {
          tracing::error!("tabs mutex poisoned: {e:?}; dropping cached browser");
          self.connected.store(false, std::sync::atomic::Ordering::SeqCst);
          return None;
        }
      };
      guard.first().cloned()
    };

    self.browser = Some(browser);
    if tab.is_none() {
      tracing::warn!("chrome reports no tabs");
    }
    tab
  }

  async fn connect_browser(&mut self) {
    let url = chrome_status_url();
    tracing::debug!("probing {url}");

    let res = match self.http.get(&url).send().await {
      Ok(r) => r,
      Err(e) => {
        tracing::warn!("failed to GET {url}: {e}");
        tokio::time::sleep(CHROME_CONNECT_BACKOFF).await;
        return;
      }
    };

    let status = match res.json::<ChromeStatus>().await {
      Ok(s) => s,
      Err(e) => {
        tracing::warn!("failed to parse {url}: {e}");
        tokio::time::sleep(CHROME_CONNECT_BACKOFF).await;
        return;
      }
    };
    tracing::trace!("chrome status: {status:?}");

    match Browser::connect_with_timeout(status.url, CHROME_IDLE_TIMEOUT) {
      Ok(browser) => {
        self.connected.store(true, std::sync::atomic::Ordering::SeqCst);
        self.browser = Some(browser);
      }
      Err(e) => {
        tracing::warn!("Browser::connect_with_timeout failed: {e:?}");
        tokio::time::sleep(CHROME_CONNECT_BACKOFF).await;
      }
    }
  }
}

fn url_matches_current(current: &str, target: &str) -> bool {
  let normalize = |s: &str| s.trim_end_matches('/').to_string();
  normalize(current) == normalize(target)
}

fn safe_call<F, R>(label: &str, f: F) -> anyhow::Result<R>
where
  F: FnOnce() -> anyhow::Result<R>,
{
  match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
    Ok(Ok(r)) => Ok(r),
    Ok(Err(e)) => Err(e),
    Err(p) => anyhow::bail!("{label} panicked: {p:?}"),
  }
}

fn catch_panic<F>(label: &str, f: F) -> anyhow::Result<()>
where
  F: FnOnce(),
{
  match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
    Ok(()) => Ok(()),
    Err(p) => anyhow::bail!("{label} panicked: {p:?}"),
  }
}

#[derive(Debug, Deserialize)]
struct ChromeStatus {
  #[serde(rename = "webSocketDebuggerUrl")]
  url: String,
}

type Result<T> = std::result::Result<T, ChromeError>;
#[derive(Debug, thiserror::Error)]
pub enum ChromeError {
  #[error("chrome connection error: {0}")]
  Connect(Box<dyn std::error::Error + Send + Sync>),
  #[error(transparent)]
  Tx(#[from] tokio::sync::mpsc::error::SendError<ChromeCommand>),
}
