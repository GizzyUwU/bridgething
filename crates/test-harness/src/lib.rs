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
use bridgething::{DaemonConfig, HeadlessInject, State};
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
use tokio::{io::DuplexStream, task::JoinHandle};
use tokio_util::codec::Framed;
use uuid::Uuid;

const DUPLEX_BUF: usize = 256 * 1024;

/// A running headless daemon plus the handles a scenario needs to drive
/// and observe it.
pub struct Harness {
  state: State,
  inject: HeadlessInject,
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
    let daemon = tokio::spawn(assembled.run());

    Ok(Self {
      state,
      inject,
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
