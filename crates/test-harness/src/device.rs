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
use futures::StreamExt;
use libbridgething::{BRIDGETHING_RFCOMM_CHANNEL, BRIDGETHING_WS_MODERN_PORT};
use tokio_tungstenite::connect_async;

use crate::{FrameObserver, MockWsClient, frame_tap_ws_observer};

const DISCOVER_TIMEOUT: Duration = Duration::from_secs(20);

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

    let stream = rfcomm::Stream::connect(rfcomm::SocketAddr::new(self.bt_addr, BRIDGETHING_RFCOMM_CHANNEL))
      .await
      .context("rfcomm connect to bridgething SPP channel")?;

    Ok(Gateway::from_io(stream))
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
