//! Tier-3 host rig: drive a real booted Car Thing over the air.
//!
//! Two concretes the seam will later unify. The over-air RFCOMM dial (real host
//! radio -> the device's BCM chip -> bluez SPP) feeds the same
//! `bridgething_gateway::Gateway` the in-process Android driver uses, and the
//! tunneled frame-tap WS feeds the same `FrameObserver`. The device address
//! comes from the environment so one rig points at whatever is on the bench.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use bluer::{Adapter, AdapterEvent, Address, Device, Session, agent::Agent, rfcomm};
use bridgething::FRAME_TAP_PORT;
use bridgething_gateway::Gateway;
use bridgething_iap2::{
  DeviceEaStream, DeviceEmulator, EmulatorEvent, IAP2_RFCOMM_CHANNEL, Iap2Command, Link, LinkConfig, Lsp,
  SessionTriple, session::EaPriority,
};
use futures::StreamExt;
use libbridgething::{BRIDGETHING_RFCOMM_CHANNEL, BRIDGETHING_WS_MODERN_PORT};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  sync::mpsc,
};
use tokio_tungstenite::connect_async;
use tokio_util::bytes::Bytes;

use crate::{FrameObserver, MockWsClient, frame_tap_ws_observer};

const DISCOVER_TIMEOUT: Duration = Duration::from_secs(20);

/// How long to wait for the daemon to reach Identified, request app
/// launch, and the emulator to open the EA gateway stream over a real
/// radio (MFi cert exchange + link timing included).
const EA_OPEN_TIMEOUT: Duration = Duration::from_secs(30);

/// A booted Car Thing reachable over the USB-gadget network (mDNS host) plus a
/// real Bluetooth radio. `SUPERBIRD_HOST` (default `bridgething.local`) is the
/// network host; `SUPERBIRD_BT_MAC` is the device's BT address.
pub struct DeviceHarness {
  host: String,
  bt_addr: Address,
}

impl DeviceHarness {
  /// Build from the environment. Errors if `SUPERBIRD_BT_MAC` is unset/malformed.
  pub fn from_env() -> Result<Self> {
    let host = std::env::var("SUPERBIRD_HOST").unwrap_or_else(|_| "bridgething.local".into());
    let raw =
      std::env::var("SUPERBIRD_BT_MAC").context("set SUPERBIRD_BT_MAC to the device BT address for over-air T3")?;
    let bt_addr = raw
      .parse::<Address>()
      .map_err(|e| anyhow::anyhow!("bad SUPERBIRD_BT_MAC {raw:?}: {e}"))?;
    Ok(Self { host, bt_addr })
  }

  pub fn new(host: impl Into<String>, bt_addr: Address) -> Self {
    Self {
      host: host.into(),
      bt_addr,
    }
  }

  /// Observe the device's egress frames over the tunneled frame-tap WS bridge.
  pub async fn frame_tap(&self) -> Result<FrameObserver> {
    frame_tap_ws_observer(&format!("ws://{}:{}/", self.host, FRAME_TAP_PORT)).await
  }

  /// Open a modern-mode WS to the device over the USB-gadget network.
  pub async fn connect_modern_client(&self) -> Result<MockWsClient> {
    let (stream, _resp) = connect_async(format!("ws://{}:{}/", self.host, BRIDGETHING_WS_MODERN_PORT)).await?;
    Ok(MockWsClient { stream })
  }

  /// Dial the device's bridgething SPP over the real radio and drive it with the
  /// real gateway SDK - the lying BCM chip in the loop. Pairs Just Works (the
  /// no-auth posture) and trusts on first run; bluez persists the bond.
  pub async fn connect_over_air(&self) -> Result<Gateway> {
    let stream = self.dial(BRIDGETHING_RFCOMM_CHANNEL).await?;
    Ok(Gateway::from_io(stream))
  }

  /// Dial the device's iAP2 accessory channel over the real radio and drive it
  /// with the device-half emulator (`Link::run_device` + `DeviceEmulator`) plus
  /// the real MFi chip on the accessory. The emulator walks auth, identification,
  /// and opens the EA gateway stream when the daemon requests app launch; the
  /// returned `Gateway` rides that EA stream, identical to the rfcomm companion
  /// once it is up. The lying BCM chip and the real CP3.0 coprocessor are both in
  /// the loop - the honest iAP2 transport test.
  pub async fn connect_over_air_iap2(&self) -> Result<Gateway> {
    let stream = self.dial(IAP2_RFCOMM_CHANNEL).await?;

    let (link_command_tx, link_command_rx) = mpsc::channel::<Iap2Command>(64);
    let (link_events_tx, link_events_rx) = mpsc::channel(64);
    tokio::spawn(Link::run_device(
      stream,
      iap2_device_config(),
      link_events_tx,
      link_command_rx,
    ));

    let (emu_events_tx, mut emu_events_rx) = mpsc::channel(64);
    tokio::spawn(DeviceEmulator::new(link_command_tx, link_events_rx, emu_events_tx).run());

    let deadline = tokio::time::Instant::now() + EA_OPEN_TIMEOUT;
    loop {
      let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
      if remaining.is_zero() {
        bail!("emulator did not open an EA gateway stream within {EA_OPEN_TIMEOUT:?}");
      }
      match tokio::time::timeout(remaining, emu_events_rx.recv()).await {
        Ok(Some(EmulatorEvent::EaStreamOpened(stream))) => return Ok(bridge_ea_stream(stream)),
        Ok(Some(_)) => continue,
        Ok(None) => bail!("emulator exited before opening an EA gateway stream"),
        Err(_) => bail!("emulator did not open an EA gateway stream within {EA_OPEN_TIMEOUT:?}"),
      }
    }
  }

  /// Just-Works pair (the no-auth posture) + trust, then dial the given RFCOMM
  /// channel. bluez persists the bond, so re-dials skip pairing.
  async fn dial(&self, channel: u8) -> Result<rfcomm::Stream> {
    let session = Session::new().await.context("open bluez session")?;
    let adapter = session.default_adapter().await.context("default bt adapter")?;
    adapter.set_powered(true).await.context("power on adapter")?;

    let agent = Agent {
      request_default: true,
      request_confirmation: Some(Box::new(|_| Box::pin(async { Ok(()) }))),
      request_authorization: Some(Box::new(|_| Box::pin(async { Ok(()) }))),
      authorize_service: Some(Box::new(|_| Box::pin(async { Ok(()) }))),
      ..Default::default()
    };
    let _agent = session
      .register_agent(agent)
      .await
      .context("register just-works agent")?;

    let device = self.ensure_known(&adapter).await?;
    if !device.is_paired().await.unwrap_or(false) {
      device.pair().await.context("pair with device (just works)")?;
    }
    let _ = device.set_trusted(true).await;

    rfcomm::Stream::connect(rfcomm::SocketAddr::new(self.bt_addr, channel))
      .await
      .with_context(|| format!("rfcomm connect to channel {channel}"))
  }

  /// Ensure bluez knows the target, discovering briefly if it does not.
  async fn ensure_known(&self, adapter: &Adapter) -> Result<Device> {
    if adapter.device_addresses().await?.contains(&self.bt_addr) {
      return Ok(adapter.device(self.bt_addr)?);
    }
    let mut events = adapter.discover_devices().await.context("start bt discovery")?;
    let deadline = tokio::time::Instant::now() + DISCOVER_TIMEOUT;
    loop {
      let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
      if remaining.is_zero() {
        bail!("device {} not discovered within {DISCOVER_TIMEOUT:?}", self.bt_addr);
      }
      match tokio::time::timeout(remaining, events.next()).await {
        Ok(Some(AdapterEvent::DeviceAdded(addr))) if addr == self.bt_addr => {
          return Ok(adapter.device(self.bt_addr)?);
        }
        Ok(Some(_)) => continue,
        Ok(None) => bail!("bt discovery stream ended before finding {}", self.bt_addr),
        Err(_) => bail!("device {} not discovered within {DISCOVER_TIMEOUT:?}", self.bt_addr),
      }
    }
  }
}

/// Device-half link config matching the real iPhone's SYN|ACK LSP
/// (max_outgoing 127, max_len 65535, sessions control/file-transfer/EA).
fn iap2_device_config() -> LinkConfig {
  let lsp = Lsp {
    version: 1,
    max_outgoing: 127,
    max_len: 65535,
    retransmission_timeout_ms: 6000,
    ack_timeout_ms: 3000,
    max_retransmissions: 30,
    max_ack: 3,
    sessions: vec![
      SessionTriple {
        id: 1,
        session_type: 0,
        version: 1,
      },
      SessionTriple {
        id: 2,
        session_type: 1,
        version: 2,
      },
      SessionTriple {
        id: 3,
        session_type: 2,
        version: 1,
      },
    ],
  };
  let mut config = LinkConfig::new(lsp);
  config.initial_psn = 215;
  config
}

/// Bridge an opened EA stream's byte channels into an `AsyncRead + AsyncWrite`
/// the gateway SDK can drive. Two pump tasks shuttle bytes between the duplex
/// and the EA stream: accessory-sent chunks become readable on the gateway end,
/// and the gateway's writes ride the EA chunker back out on link session 3.
fn bridge_ea_stream(stream: DeviceEaStream) -> Gateway {
  let DeviceEaStream {
    mut inbound_rx,
    outbound,
    ..
  } = stream;
  let (gateway_io, emulator_io) = tokio::io::duplex(64 * 1024);
  let (mut emu_rd, mut emu_wr) = tokio::io::split(emulator_io);

  tokio::spawn(async move {
    while let Some(chunk) = inbound_rx.recv().await {
      if emu_wr.write_all(&chunk).await.is_err() {
        break;
      }
    }
  });

  tokio::spawn(async move {
    let mut buf = vec![0u8; 8192];
    loop {
      match emu_rd.read(&mut buf).await {
        Ok(0) | Err(_) => break,
        Ok(n) => {
          if outbound
            .send(EaPriority::Normal, Bytes::copy_from_slice(&buf[..n]))
            .await
            .is_err()
          {
            break;
          }
        }
      }
    }
  });

  Gateway::from_io(gateway_io)
}
