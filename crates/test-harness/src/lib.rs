//! In-process test harness for bridgething. Assembles a headless daemon
//! (no BlueZ) and drives it through a mock phone whose bytes are exactly
//! what a real RFCOMM companion would send - the only difference is the
//! byte stream is a `tokio::io::duplex` pair instead of a Bluetooth
//! socket. Assertions read daemon state directly through the public
//! `AppState` surface.

use std::{
  path::Path,
  sync::atomic::{AtomicU8, Ordering},
  time::Duration,
};

mod chrome;
mod device;
use anyhow::Result;
use bluer::Address;
use bridgething::{DaemonConfig, HeadlessInject, Iap2Event, ServerAddrs, State, TappedFrame};
use bridgething_gateway::Gateway;
use bridgething_iap2::SessionEvent;
pub use chrome::ChromeView;
pub use device::DeviceHarness;
use futures::StreamExt;
use tokio::{net::TcpStream, sync::broadcast, task::JoinHandle};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

const DUPLEX_BUF: usize = 256 * 1024;
const FRAME_OBSERVER_CAPACITY: usize = 256;

/// A running headless daemon plus the handles a scenario needs to drive
/// and observe it.
pub struct Harness {
  state: State,
  inject: HeadlessInject,
  server_addrs: ServerAddrs,
  next_peer: AtomicU8,
  _daemon: JoinHandle<()>,
  _state_dir: tempfile::TempDir,
}

impl Harness {
  /// Assemble a fresh headless daemon with an isolated in-memory db and
  /// temp blob stores, then run its event loop in the background.
  pub async fn start() -> Result<Self> {
    Self::start_inner(false).await
  }

  /// Like `start`, but provisions the real stock SPA bundle as the headless
  /// daemon's active webapp (served on the modern http port) so a Tier-2
  /// `ChromeView` can render it. Errors when the stock dist is not on disk.
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

  /// Direct read access to daemon state for assertions.
  pub fn state(&self) -> &State {
    &self.state
  }

  /// Inject a duplex into the daemon's rfcomm channel and drive the other half
  /// with the real gateway SDK. Dropping the returned [`Gateway`] disconnects.
  pub async fn connect_android(&self) -> Result<Gateway> {
    let (daemon_half, phone_half) = tokio::io::duplex(DUPLEX_BUF);
    let n = self.next_peer.fetch_add(1, Ordering::Relaxed);
    let addr = Address([0xFE, 0xED, 0x00, 0x00, 0x00, n]);
    self.inject.rfcomm.send((addr, daemon_half)).await?;
    Ok(Gateway::from_io(phone_half))
  }

  /// Inject a raw iAP2 `SessionEvent` into the daemon's event channel,
  /// bypassing the link, session, and MFi layers.
  pub async fn inject_iap2(&self, address: Address, event: SessionEvent) -> Result<()> {
    self.inject.iap2.send(Iap2Event { address, event }).await?;
    Ok(())
  }

  /// Deliver iAP2 artwork bytes for `transfer_id`, as the daemon would receive after a now-playing delta.
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

  /// Convenience: an iAP2 peer address distinct from the Android range, so
  /// scenarios mixing both transports get non-colliding peers.
  pub fn iap2_peer(&self) -> Address {
    let n = self.next_peer.fetch_add(1, Ordering::Relaxed);
    Address([0xA9, 0x00, 0x00, 0x00, 0x00, n])
  }

  /// Poll a predicate against daemon state until it holds or the timeout
  /// elapses. The daemon applies wire messages asynchronously, so state
  /// assertions converge rather than being instantaneous.
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

  /// Subscribe to the daemon's egress frame mirror. Only frames sent after this call arrive.
  pub fn observe_frames(&self) -> FrameObserver {
    FrameObserver {
      rx: self.state.client_man.subscribe_frames(),
    }
  }

  /// Open a modern-mode websocket to the daemon's bound modern port.
  pub async fn connect_modern_client(&self) -> Result<MockWsClient> {
    let (stream, _resp) = connect_async(format!("ws://{}/", self.server_addrs.modern)).await?;
    Ok(MockWsClient { stream })
  }

  /// Open a stock-mode websocket to the daemon's bound stock port.
  pub async fn connect_stock_client(&self) -> Result<MockWsClient> {
    let (stream, _resp) = connect_async(format!("ws://{}/", self.server_addrs.stock)).await?;
    Ok(MockWsClient { stream })
  }

  /// Launch headless chromium against the daemon and render the real stock SPA
  /// (Tier-2 observer). Requires `start_with_stock_webapp`. Errors when no
  /// chromium binary is present, which lets callers skip rather than fail.
  pub async fn open_stock_chrome(&self) -> Result<ChromeView> {
    ChromeView::launch(self.server_addrs.modern, self.server_addrs.stock.port()).await
  }

  /// Observe egress frames via the daemon's frame-tap WS bridge rather than the in-process broadcast.
  pub async fn connect_frame_tap_ws(&self) -> Result<FrameObserver> {
    frame_tap_ws_observer(&format!("ws://{}/", self.server_addrs.frame_tap)).await
  }
}

/// Ergonomic view over the egress frame mirror. Wraps the broadcast receiver
/// with predicate-wait and windowed-collect helpers scenarios assert against.
pub struct FrameObserver {
  rx: broadcast::Receiver<TappedFrame>,
}

impl FrameObserver {
  /// Wait for an egress frame matching `pred`, or None on timeout. Lagged
  /// frames (a slow observer falling behind the broadcast ring) are skipped.
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

  /// Collect every frame observed over `window`. Use to assert on an ordered
  /// sequence (e.g. an art id that must never revert to absent).
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

/// Connect to a daemon's frame-tap WS bridge and wrap it in a [`FrameObserver`].
/// A background task deserializes inbound frames and re-publishes them on a local broadcast;
/// the observer sees channel-close when the WS connection drops.
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

/// A real websocket client against the daemon's client server. Holds the
/// stream open so the daemon keeps the connection; `recv` reads the next
/// text frame the daemon sent.
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
}

/// Symlink the real stock SPA dist into the headless daemon's builtin webapp
/// root as the reserved `stock` bundle, so the registry's startup scan picks it
/// up with the synthetic stock manifest and `active_webapp` falls through to it.
fn provision_stock_bundle(state_dir: &Path) -> Result<()> {
  let dist = stock_dist_path()?;
  let builtin = state_dir.join("builtin");
  std::fs::create_dir_all(&builtin)?;
  std::os::unix::fs::symlink(&dist, builtin.join("stock"))?;
  Ok(())
}

/// Locate the built stock SPA. `BRIDGETHING_STOCK_DIST` overrides; otherwise the
/// sibling `superbird-webapp/dist` checkout relative to this crate. Errors when
/// absent so Tier-2 callers can skip on a runner that has not built it.
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
