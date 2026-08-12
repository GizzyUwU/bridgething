use std::time::Duration;

use anyhow::{Context, Result, bail};
use bluer::{Adapter, AdapterEvent, Address, Device, Session, agent::Agent, rfcomm};
use bridgething::FRAME_TAP_PORT;
use bridgething_gateway::Gateway;
use bridgething_iap2::{
  DeviceEaStream, DeviceEmulator, DeviceEmulatorHandle, EmulatorEvent, IAP2_RFCOMM_CHANNEL, Iap2Command, Iap2Event,
  Link, LinkConfig, Lsp, SessionTriple,
};
use futures::StreamExt;
use libbridgething::{BRIDGETHING_RFCOMM_CHANNEL, BRIDGETHING_WS_MODERN_PORT, Priority};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  sync::mpsc,
};
use tokio_tungstenite::connect_async;
use tokio_util::bytes::Bytes;

use crate::{FrameObserver, MockWsClient, frame_tap_ws_observer};

const DISCOVER_TIMEOUT: Duration = Duration::from_secs(20);
const EA_OPEN_TIMEOUT: Duration = Duration::from_secs(30);

pub struct DeviceHarness {
  host: String,
  bt_addr: Address,
}

impl DeviceHarness {
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

  pub async fn frame_tap(&self) -> Result<FrameObserver> {
    frame_tap_ws_observer(&format!("ws://{}:{}/", self.host, FRAME_TAP_PORT)).await
  }

  pub async fn connect_modern_client(&self) -> Result<MockWsClient> {
    let (stream, _resp) = connect_async(format!("ws://{}:{}/", self.host, BRIDGETHING_WS_MODERN_PORT)).await?;
    Ok(MockWsClient { stream })
  }

  pub async fn connect_over_air(&self) -> Result<Gateway> {
    let stream = self.dial(BRIDGETHING_RFCOMM_CHANNEL).await?;
    Ok(Gateway::from_io(stream))
  }

  pub async fn connect_over_air_extra(&self) -> Result<Gateway> {
    let stream = self.dial_extra(BRIDGETHING_RFCOMM_CHANNEL).await?;
    Ok(Gateway::from_io(stream))
  }

  pub async fn connect_over_air_iap2(&self) -> Result<Gateway> {
    let (link_command_tx, link_events_rx) = self.dial_iap2().await?;
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

  pub async fn connect_iap2_emulator(&self) -> Result<DeviceEmulatorHandle> {
    let (link_command_tx, link_events_rx) = self.dial_iap2().await?;
    let (emu_events_tx, mut emu_events_rx) = mpsc::channel(64);
    let emulator = DeviceEmulator::new(link_command_tx, link_events_rx, emu_events_tx).without_now_playing();
    let handle = emulator.handle();
    tokio::spawn(emulator.run());

    let deadline = tokio::time::Instant::now() + EA_OPEN_TIMEOUT;
    loop {
      let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
      if remaining.is_zero() {
        bail!("emulator did not reach identification within {EA_OPEN_TIMEOUT:?}");
      }
      match tokio::time::timeout(remaining, emu_events_rx.recv()).await {
        Ok(Some(EmulatorEvent::Identified)) => break,
        Ok(Some(_)) => continue,
        Ok(None) => bail!("emulator exited before identification"),
        Err(_) => bail!("emulator did not reach identification within {EA_OPEN_TIMEOUT:?}"),
      }
    }
    tokio::spawn(async move { while emu_events_rx.recv().await.is_some() {} });
    Ok(handle)
  }

  async fn dial_iap2(&self) -> Result<(mpsc::Sender<Iap2Command>, mpsc::Receiver<Iap2Event>)> {
    let stream = self.dial(IAP2_RFCOMM_CHANNEL).await?;
    let (link_command_tx, link_command_rx) = mpsc::channel::<Iap2Command>(64);
    let (link_events_tx, link_events_rx) = mpsc::channel(64);
    tokio::spawn(Link::run_device(
      stream,
      iap2_device_config(),
      link_events_tx,
      link_command_rx,
    ));
    Ok((link_command_tx, link_events_rx))
  }

  async fn dial(&self, channel: u8) -> Result<rfcomm::Stream> {
    self.dial_inner(channel, true).await
  }

  pub async fn dial_extra(&self, channel: u8) -> Result<rfcomm::Stream> {
    self.dial_inner(channel, false).await
  }

  async fn dial_inner(&self, channel: u8, drop_acl_first: bool) -> Result<rfcomm::Stream> {
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

    if drop_acl_first {
      let _ = device.disconnect().await;
      tokio::time::sleep(Duration::from_millis(500)).await;
    }

    rfcomm::Stream::connect(rfcomm::SocketAddr::new(self.bt_addr, channel))
      .await
      .with_context(|| format!("rfcomm connect to channel {channel}"))
  }

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
            .send(Priority::Normal, Bytes::copy_from_slice(&buf[..n]))
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
