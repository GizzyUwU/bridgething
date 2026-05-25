//! In-process test harness for bridgething. Assembles a headless daemon
//! (no BlueZ) and drives it through a mock phone whose bytes are exactly
//! what a real RFCOMM companion would send - the only difference is the
//! byte stream is a `tokio::io::duplex` pair instead of a Bluetooth
//! socket. Assertions read daemon state directly through the public
//! `AppState` surface.

use std::{
  sync::atomic::{AtomicU8, Ordering},
  time::Duration,
};

use anyhow::Result;
use bluer::Address;
use bridgething::{DaemonConfig, HeadlessInject, Iap2Event, ServerAddrs, State, TappedFrame};
use bridgething_iap2::SessionEvent;
use futures::{SinkExt, StreamExt};
use libbridgething::{
  AssetRetention, CompanionAuthorityScope, GatewayCapabilities, GatewayInfo, NowPlayingUpdate,
  gateway::{
    AssetPush, AuthorityClaim, BridgeToGatewayMsg, GatewayToBridgeAssetMsg, GatewayToBridgeAuthorityMsg,
    GatewayToBridgeCapabilitiesMsg, GatewayToBridgeMsg, GatewayToBridgeMsgData, GatewayToBridgePlayerMsg,
  },
  protocol::GatewayEndec,
  wire::MsgMeta,
};
use tokio::{io::DuplexStream, net::TcpStream, sync::broadcast, task::JoinHandle};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};
use tokio_util::codec::Framed;
use uuid::Uuid;

const DUPLEX_BUF: usize = 256 * 1024;

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
    let state_dir = tempfile::tempdir()?;
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

  /// Attach a mock Android companion: open a duplex, hand the daemon one
  /// end through the rfcomm inject channel (it sees a `GatewayType::Rfcomm`
  /// peer indistinguishable from a real one), and drive the other end.
  pub async fn connect_android(&self) -> Result<MockPhone> {
    let (daemon_half, phone_half) = tokio::io::duplex(DUPLEX_BUF);
    let n = self.next_peer.fetch_add(1, Ordering::Relaxed);
    let addr = Address([0xFE, 0xED, 0x00, 0x00, 0x00, n]);
    self.inject.rfcomm.send((addr, daemon_half)).await?;
    Ok(MockPhone::new(phone_half, addr))
  }

  /// Inject a raw iAP2 `SessionEvent` for `address` into the same channel
  /// the real `Iap2Manager` feeds. The unmodified `Iap2EventRouter` routes
  /// it through the real Player merge / stock translation / broadcast - no
  /// Link, no session, no MFi. This is the iOS T1/T2 driver: the session
  /// events are what a real iPhone's iAP2 session produces, minus the radio.
  pub async fn inject_iap2(&self, address: Address, event: SessionEvent) -> Result<()> {
    self.inject.iap2.send(Iap2Event { address, event }).await?;
    Ok(())
  }

  /// Deliver iAP2 artwork bytes (the FileTransfer the daemon pairs to a
  /// prior now-playing delta's `artwork_id`). On a real iPhone these arrive
  /// a beat after the metadata - the latency the cover-art bug rides.
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

  /// Observe the daemon's egress frame mirror. Each frame is one
  /// post-translation message the daemon sent to a client connection - the
  /// observer side of the flicker bug, where the symptom is a broadcast
  /// stream that oscillates even when settled state looks clean. Start
  /// observing before driving a scenario; only frames sent afterward arrive.
  pub fn observe_frames(&self) -> FrameObserver {
    FrameObserver {
      rx: self.state.client_man.subscribe_frames(),
    }
  }

  /// Open a real modern-mode websocket to the daemon's bound modern port.
  /// The daemon proactively pushes a capabilities snapshot to every new
  /// modern client, so a connect alone produces an observable egress frame.
  pub async fn connect_modern_client(&self) -> Result<MockWsClient> {
    let (stream, _resp) = connect_async(format!("ws://{}/", self.server_addrs.modern)).await?;
    Ok(MockWsClient { stream })
  }

  /// Open a real stock-mode websocket to the daemon's bound stock port. The
  /// daemon broadcasts stock-translated now-playing to it like any client,
  /// so a bare stock connection observes the merge/re-broadcast suspects.
  /// (The serve-asset suspect is request-driven and needs the real SPA - T2.)
  pub async fn connect_stock_client(&self) -> Result<MockWsClient> {
    let (stream, _resp) = connect_async(format!("ws://{}/", self.server_addrs.stock)).await?;
    Ok(MockWsClient { stream })
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

/// A mock phone speaking the gateway wire protocol over a byte stream.
/// Android mode only for now; the same speaker layers under an iAP2 peer
/// for iOS once that lane lands.
pub struct MockPhone {
  framed: Framed<DuplexStream, GatewayEndec>,
  addr: Address,
}

impl MockPhone {
  fn new(stream: DuplexStream, addr: Address) -> Self {
    Self {
      framed: Framed::new(stream, GatewayEndec::default()),
      addr,
    }
  }

  pub fn address(&self) -> Address {
    self.addr
  }

  async fn send(&mut self, data: GatewayToBridgeMsgData) -> Result<()> {
    let msg = GatewayToBridgeMsg {
      id: Uuid::now_v7(),
      meta: MsgMeta::Event,
      data,
    };
    self.framed.send(msg).await?;
    Ok(())
  }

  /// Announce capabilities. The daemon upserts the peer into PeerTracker
  /// and marks the companion connected (the Android useful-link path).
  pub async fn announce(&mut self) -> Result<()> {
    let caps = GatewayCapabilities {
      gateway: GatewayInfo {
        address: String::new(),
        name: "mock-android".into(),
        os_name: "android".into(),
        app_name: "mock-android".into(),
        app_version: "0.0.0".into(),
        adapter_version: "mock".into(),
        lib_version: "0.0.0".into(),
        libbridgething_version: format!("v{}", libbridgething::LIBBRIDGETHING_VERSION),
      },
      ..Default::default()
    };
    self.send(GatewayToBridgeCapabilitiesMsg::Announce(caps).into()).await
  }

  /// Claim a now-playing authority scope. Companion data only surfaces in
  /// merged player state for scopes the companion holds.
  pub async fn claim_authority(&mut self, scope: CompanionAuthorityScope) -> Result<()> {
    self
      .send(GatewayToBridgeAuthorityMsg::Claim(AuthorityClaim { scope }).into())
      .await
  }

  pub async fn now_playing(&mut self, update: NowPlayingUpdate) -> Result<()> {
    self.send(GatewayToBridgePlayerMsg::Delta(update).into()).await
  }

  pub async fn push_asset(&mut self, id: &str, retention: AssetRetention, bytes: Vec<u8>) -> Result<()> {
    self
      .send(
        GatewayToBridgeAssetMsg::Push(AssetPush {
          id: id.into(),
          bytes,
          mime: Some("image/jpeg".into()),
          retention,
        })
        .into(),
      )
      .await
  }

  /// Next message the daemon sent to this phone, or None if the link
  /// closed. The daemon sends a version event on connect; callers that
  /// assert on outbound traffic should account for it.
  pub async fn recv(&mut self) -> Option<BridgeToGatewayMsg> {
    match self.framed.next().await {
      Some(Ok(frame)) => Some(frame.msg),
      _ => None,
    }
  }
}
