use std::sync::{Arc, atomic::AtomicBool};

use headless_chrome::Browser;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

const CHROME_PORT: u16 = 9222;
const CHROME_STATUS_URL: &str = "http://127.0.1:9222/json/version";
const CHROME_WS_URL: &str = "ws://127.0.1:9222/devtools/browser/{}";

type ChromeTx = tokio::sync::mpsc::Sender<ChromeCommand>;
type ChromeRx = tokio::sync::mpsc::Receiver<ChromeCommand>;

#[derive(Debug, Clone)]
pub enum ChromeCommand {
  Navigate(String),
}

#[derive(Debug)]
pub struct Chrome {
  connected: Arc<AtomicBool>,
  tx: ChromeTx,

  cancel_token: tokio_util::sync::CancellationToken,
  _worker: tokio::task::JoinHandle<()>,
}

impl Chrome {
  pub async fn init() -> Result<Self> {
    tracing::debug!("initializing chrome worker");
    let (tx, rx) = tokio::sync::mpsc::channel(16);
    let connected = Arc::new(AtomicBool::new(false));

    let cancel_token = tokio_util::sync::CancellationToken::new();
    let mut worker = ChromeWorker::new(connected.clone(), rx, cancel_token.clone());

    Ok(Self {
      connected: connected.clone(),
      tx,

      cancel_token: cancel_token.clone(),
      _worker: tokio::spawn(async move { worker.run().await }),
    })
  }

  pub fn connected(&self) -> bool {
    self.connected.load(std::sync::atomic::Ordering::SeqCst)
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
  browser: Option<Browser>,

  rx: ChromeRx,
  cancel_token: CancellationToken,
}

impl ChromeWorker {
  pub fn new(connected: Arc<AtomicBool>, rx: ChromeRx, cancel_token: CancellationToken) -> Self {
    Self {
      connected,
      browser: None,

      rx,
      cancel_token,
    }
  }

  pub async fn run(&mut self) {
    loop {
      tokio::select! {
        Some(message) = self.rx.recv() => {
          match message {
            ChromeCommand::Navigate(url) => self.handle_navigate(url).await,
          }
        }
        _ = self.cancel_token.cancelled() => {
          tracing::debug!("chrome worker shutting down");
          break;
        }
      }
    }
  }

  async fn handle_navigate(&mut self, url: String) {
    tracing::debug!("navigating to {}", url);

    let Some(browser) = self.get_connection().await else {
      tracing::warn!("chrome not connected");
      return;
    };

    browser.register_missing_tabs();
    let Some(tab) = (if let Ok(tabs) = browser.get_tabs().lock() {
      tabs.first().cloned()
    } else {
      tracing::error!("tab mutex poisoned!!");
      return;
    }) else {
      tracing::error!("no tabs found!");
      return;
    };

    if let Err(e) = tab.navigate_to(&url) {
      tracing::error!("failed to navigate to {}: {}", url, e);
    }
  }

  async fn get_connection(&mut self) -> &Option<Browser> {
    tracing::debug!("getting chrome connection");
    if self.browser.is_some() {
      return &self.browser;
    }

    let Ok(res) = reqwest::get(CHROME_STATUS_URL).await else {
      tracing::error!("failed to connect to chrome");
      tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
      return &None;
    };

    let Ok(status) = res.json::<ChromeStatus>().await else {
      tracing::error!("failed to parse chrome status");
      tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
      return &None;
    };
    tracing::trace!("chrome status: {:?}", &status);

    if let Ok(c) = Browser::connect(status.url) {
      self.connected.store(true, std::sync::atomic::Ordering::SeqCst);
      self.browser = Some(c);
      &self.browser
    } else {
      &None
    }
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
  Connect(Box<dyn std::error::Error>),
  #[error(transparent)]
  Tx(#[from] tokio::sync::mpsc::error::SendError<ChromeCommand>),
}
