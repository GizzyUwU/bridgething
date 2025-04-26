use std::sync::{Arc, atomic::AtomicBool};

use headless_chrome::Browser;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

const CHROME_PORT: u16 = 9222;
const CHROME_STATUS_URL: &str = "http://127.0.1:9222/json/version";
const CHROME_WS_URL: &str = "ws://127.0.1:9222/devtools/browser/{}";

type ChromeTx = tokio::sync::mpsc::Sender<ChromeCommand>;
type ChromeRx = tokio::sync::mpsc::Receiver<ChromeCommand>;

pub type Chrome = Arc<ChromeManager>;

#[derive(Debug, Clone)]
pub enum ChromeCommand {}

pub struct ChromeManager {
  connected: Arc<AtomicBool>,
  tx: ChromeTx,

  cancel_token: tokio_util::sync::CancellationToken,
  _worker: tokio::task::JoinHandle<()>,
}

impl ChromeManager {
  pub async fn init() -> Result<Arc<Self>> {
    let c = Browser::connect("ws://127.0.0.1:9222/devtools/browser/c0759d46-59d3-44fe-afd6-77c9dbc47615".to_string())
      .map_err(|e| {
      tracing::error!("failed to connect to chrome: {:?}", e);
      ChromeError::Connect(e.into_boxed_dyn_error())
    })?;

    let (tx, rx) = tokio::sync::mpsc::channel(16);
    let connected = Arc::new(AtomicBool::new(false));

    let cancel_token = tokio_util::sync::CancellationToken::new();
    let mut worker = ChromeWorker::new(connected.clone(), rx, cancel_token.clone());

    Ok(Arc::new(Self {
      connected: connected.clone(),
      tx,

      cancel_token: cancel_token.clone(),
      _worker: tokio::spawn(async move { worker.run().await }),
    }))
  }

  pub fn connected(&self) -> bool {
    self.connected.load(std::sync::atomic::Ordering::SeqCst)
  }

  pub async fn send(&self, command: ChromeCommand) -> Result<()> {
    Ok(self.tx.send(command).await?)
  }

  pub async fn shutdown(&self) {
    self.cancel_token.cancel();
  }
}

struct ChromeWorker {
  connected: Arc<AtomicBool>,
  rx: ChromeRx,
  cancel_token: CancellationToken,
}

impl ChromeWorker {
  pub fn new(connected: Arc<AtomicBool>, rx: ChromeRx, cancel_token: CancellationToken) -> Self {
    Self {
      connected,
      rx,
      cancel_token,
    }
  }

  pub async fn run(&mut self) {
    loop {
      tokio::select! {
        Some(_) = self.rx.recv() => {
          // handle message
        }
        _ = self.cancel_token.cancelled() => {
          tracing::debug!("chrome worker shutting down");
          break;
        }
      }
    }
  }

  async fn connect_loop(&self) -> Option<()> {
    loop {
      let Ok(res) = reqwest::get(CHROME_STATUS_URL).await else {
        tracing::error!("failed to connect to chrome");
        tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
        continue;
      };

      let Ok(status) = res.json::<ChromeStatus>().await else {
        tracing::error!("failed to parse chrome status");
        tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
        continue;
      };

      if let Ok(c) = Browser::connect(CHROME_WS_URL.to_string()) {
        self.connected.store(true, std::sync::atomic::Ordering::SeqCst);
        return Some(());
      }
      tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
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
