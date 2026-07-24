use std::{
  path::Path,
  sync::atomic::{AtomicU8, Ordering},
  time::Duration,
};

mod chrome;
mod device;
pub mod model;
mod seam;
use anyhow::Result;
use bluer::Address;
use bridgething::{DaemonConfig, HeadlessInject, Iap2Event, Iap2TransportCommand, ServerAddrs, State, TappedFrame};
use bridgething_client::Client as CommandClient;
use bridgething_gateway::Gateway;
use bridgething_iap2::SessionEvent;
pub use chrome::ChromeView;
pub use device::DeviceHarness;
use futures::StreamExt;
pub use seam::{
  CommandDriver, DeviceIap2Source, DeviceTier, FrameObserve, GatewayDriver, HarnessIap2Source, Iap2OutboundObserve,
  Iap2Source, Iap2SourceDriver, ModernClientDriver, OverAirTransport,
};
use tokio::{net::TcpStream, sync::broadcast, task::JoinHandle};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

const DUPLEX_BUF: usize = 256 * 1024;
const FRAME_OBSERVER_CAPACITY: usize = 256;

pub struct Harness {
  state: State,
  inject: HeadlessInject,
  server_addrs: ServerAddrs,
  next_peer: AtomicU8,
  _daemon: JoinHandle<()>,
  _state_dir: tempfile::TempDir,
}

impl Harness {
  pub async fn start() -> Result<Self> {
    Self::start_inner(false).await
  }

  pub async fn start_with_stock_webapp() -> Result<Self> {
    Self::start_inner(true).await
  }

  async fn start_inner(provision_stock: bool) -> Result<Self> {
    let state_dir = tempfile::tempdir()?;
    if provision_stock {
      provision_stock_bundle(state_dir.path())?;
    }
    let assembled = bridgething::init(DaemonConfig::headless(state_dir.path().to_path_buf())).await;

    let state = assembled.state.clone();
    let inject = assembled
      .inject
      .clone()
      .expect("headless assembly must expose inject handles");
    let server_addrs = assembled.server_addrs;
    let daemon = tokio::spawn(assembled.run());

    Ok(Self {
      state,
      inject,
      server_addrs,
      next_peer: AtomicU8::new(1),
      _daemon: daemon,
      _state_dir: state_dir,
    })
  }

  pub fn state(&self) -> &State {
    &self.state
  }

  pub fn state_dir(&self) -> &Path {
    self._state_dir.path()
  }

  pub async fn restart(mut self) -> Result<Self> {
    self._daemon.abort();
    let _ = (&mut self._daemon).await;
    let assembled = bridgething::init(DaemonConfig::headless(self._state_dir.path().to_path_buf())).await;
    self.state = assembled.state.clone();
    self.inject = assembled
      .inject
      .clone()
      .expect("headless assembly must expose inject handles");
    self.server_addrs = assembled.server_addrs;
    self._daemon = tokio::spawn(assembled.run());
    Ok(self)
  }

  pub async fn connect_android(&self) -> Result<Gateway> {
    let (daemon_half, phone_half) = tokio::io::duplex(DUPLEX_BUF);
    let n = self.next_peer.fetch_add(1, Ordering::Relaxed);
    let addr = Address([0xFE, 0xED, 0x00, 0x00, 0x00, n]);
    self.inject.rfcomm.send((addr, daemon_half)).await?;
    Ok(Gateway::from_io(phone_half))
  }

  pub async fn inject_iap2(&self, address: Address, event: SessionEvent) -> Result<()> {
    self.inject.iap2.send(Iap2Event { address, event }).await?;
    Ok(())
  }

  pub async fn iap2_artwork(&self, address: Address, transfer_id: u8, bytes: Vec<u8>) -> Result<()> {
    self
      .inject_iap2(
        address,
        SessionEvent::ArtworkBytes {
          transfer_id,
          bytes: bytes.into(),
        },
      )
      .await
  }

  pub fn iap2_peer(&self) -> Address {
    let n = self.next_peer.fetch_add(1, Ordering::Relaxed);
    Address([0xA9, 0x00, 0x00, 0x00, 0x00, n])
  }

  pub async fn wait_for<F>(&self, mut predicate: F, timeout: Duration) -> bool
  where
    F: FnMut(&State) -> bool,
  {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
      if predicate(&self.state) {
        return true;
      }
      if tokio::time::Instant::now() >= deadline {
        return false;
      }
      tokio::time::sleep(Duration::from_millis(5)).await;
    }
  }

  pub fn observe_frames(&self) -> FrameObserver {
    FrameObserver {
      rx: self.state.client_man.subscribe_frames(),
    }
  }

  pub async fn connect_modern_client(&self) -> Result<MockWsClient> {
    let (stream, _resp) = connect_async(format!("ws://{}/", self.server_addrs.modern)).await?;
    Ok(MockWsClient { stream })
  }

  pub async fn connect_command_client(&self) -> Result<CommandClient> {
    Ok(CommandClient::connect(&format!("ws://{}/", self.server_addrs.modern)).await?)
  }

  pub fn observe_iap2_outbound(&self) -> Iap2OutboundObserver {
    Iap2OutboundObserver {
      rx: self.inject.iap2_outbound.subscribe(),
    }
  }

  pub async fn connect_stock_client(&self) -> Result<MockWsClient> {
    let (stream, _resp) = connect_async(format!("ws://{}/", self.server_addrs.stock)).await?;
    Ok(MockWsClient { stream })
  }

  pub fn modern_addr(&self) -> std::net::SocketAddr {
    self.server_addrs.modern
  }

  pub async fn open_stock_chrome(&self) -> Result<ChromeView> {
    ChromeView::launch(self.server_addrs.modern, self.server_addrs.stock.port()).await
  }

  pub async fn connect_frame_tap_ws(&self) -> Result<FrameObserver> {
    frame_tap_ws_observer(&format!("ws://{}/", self.server_addrs.frame_tap)).await
  }
}

pub struct FrameObserver {
  rx: broadcast::Receiver<TappedFrame>,
}

impl FrameObserver {
  pub async fn wait_for<F>(&mut self, timeout: Duration, pred: F) -> Option<TappedFrame>
  where
    F: Fn(&TappedFrame) -> bool,
  {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
      let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
      if remaining.is_zero() {
        return None;
      }
      match tokio::time::timeout(remaining, self.rx.recv()).await {
        Ok(Ok(frame)) if pred(&frame) => return Some(frame),
        Ok(Ok(_)) => continue,
        Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
        Ok(Err(broadcast::error::RecvError::Closed)) | Err(_) => return None,
      }
    }
  }

  pub async fn collect_for(&mut self, window: Duration) -> Vec<TappedFrame> {
    let mut frames = Vec::new();
    let deadline = tokio::time::Instant::now() + window;
    loop {
      let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
      if remaining.is_zero() {
        return frames;
      }
      match tokio::time::timeout(remaining, self.rx.recv()).await {
        Ok(Ok(frame)) => frames.push(frame),
        Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
        Ok(Err(broadcast::error::RecvError::Closed)) | Err(_) => return frames,
      }
    }
  }
}

pub struct Iap2OutboundObserver {
  rx: broadcast::Receiver<Iap2TransportCommand>,
}

impl Iap2OutboundObserver {
  pub async fn wait_for<F>(&mut self, timeout: Duration, pred: F) -> Option<Iap2TransportCommand>
  where
    F: Fn(&Iap2TransportCommand) -> bool,
  {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
      let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
      if remaining.is_zero() {
        return None;
      }
      match tokio::time::timeout(remaining, self.rx.recv()).await {
        Ok(Ok(cmd)) if pred(&cmd) => return Some(cmd),
        Ok(Ok(_)) => continue,
        Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
        Ok(Err(broadcast::error::RecvError::Closed)) | Err(_) => return None,
      }
    }
  }

  pub async fn collect_for(&mut self, window: Duration) -> Vec<Iap2TransportCommand> {
    let mut cmds = Vec::new();
    let deadline = tokio::time::Instant::now() + window;
    loop {
      let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
      if remaining.is_zero() {
        return cmds;
      }
      match tokio::time::timeout(remaining, self.rx.recv()).await {
        Ok(Ok(cmd)) => cmds.push(cmd),
        Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
        Ok(Err(broadcast::error::RecvError::Closed)) | Err(_) => return cmds,
      }
    }
  }
}

pub async fn frame_tap_ws_observer(url: &str) -> Result<FrameObserver> {
  let (stream, _resp) = connect_async(url).await?;
  let (tx, _seed_rx) = broadcast::channel(FRAME_OBSERVER_CAPACITY);
  let rx = tx.subscribe();

  tokio::spawn(async move {
    let mut stream = stream;
    while let Some(msg) = stream.next().await {
      match msg {
        Ok(Message::Text(text)) => match serde_json::from_str::<TappedFrame>(text.as_str()) {
          Ok(frame) => {
            let _ = tx.send(frame);
          }
          Err(err) => eprintln!("frame-tap WS observer failed to decode a frame: {err}"),
        },
        Ok(Message::Close(_)) | Err(_) => break,
        Ok(_) => continue,
      }
    }
  });

  Ok(FrameObserver { rx })
}

pub struct MockWsClient {
  stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl MockWsClient {
  pub async fn recv(&mut self) -> Option<String> {
    while let Some(msg) = self.stream.next().await {
      match msg {
        Ok(Message::Text(text)) => return Some(text.to_string()),
        Ok(_) => continue,
        Err(_) => return None,
      }
    }
    None
  }

  pub async fn send_text(&mut self, text: impl Into<String>) -> Result<()> {
    use futures::SinkExt;
    self.stream.send(Message::Text(text.into().into())).await?;
    Ok(())
  }
}

fn provision_stock_bundle(state_dir: &Path) -> Result<()> {
  let dist = stock_dist_path()?;
  let builtin = state_dir.join("builtin");
  std::fs::create_dir_all(&builtin)?;
  std::os::unix::fs::symlink(&dist, builtin.join("stock"))?;
  Ok(())
}

fn stock_dist_path() -> Result<std::path::PathBuf> {
  let has_index = |p: &Path| p.join("index.html").is_file();
  if let Ok(env) = std::env::var("BRIDGETHING_STOCK_DIST") {
    let p = std::path::PathBuf::from(env);
    if has_index(&p) {
      return Ok(p);
    }
    anyhow::bail!("BRIDGETHING_STOCK_DIST set to {} but no index.html there", p.display());
  }
  let sibling = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../superbird-webapp/dist");
  let sibling = sibling.canonicalize().unwrap_or(sibling);
  if has_index(&sibling) {
    return Ok(sibling);
  }
  anyhow::bail!(
    "stock dist not found at {} (build superbird-webapp or set BRIDGETHING_STOCK_DIST)",
    sibling.display()
  )
}
