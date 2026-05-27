//! The Driver/Observer seam: capability traits a scenario body binds on, so
//! one body lifts to every tier that satisfies its bounds.
//!
//! A tier advertises capabilities by implementing these traits; a scenario is
//! generic over the ones it needs and the per-tier test wrapper instantiates
//! it. Capabilities a tier lacks (AppState off-device, CDP over the tunnel)
//! are simply not implemented there, so a scenario needing them does not lift.
//!
//! The gateway companion is uniform: the same [`Gateway`] rides the T1
//! in-process duplex and both T3 over-air transports, so [`DeviceTier`] selects
//! the transport while one scenario body covers all of them.

use anyhow::Result;
use async_trait::async_trait;
use bluer::Address;
use bridgething::{Iap2Event, Iap2InjectTx};
use bridgething_client::Client as CommandClient;
use bridgething_gateway::Gateway;
use bridgething_iap2::{DeviceEmulatorHandle, SessionEvent, csm::now_playing::NowPlayingUpdate as Iap2NowPlaying};

use crate::{DeviceHarness, FrameObserver, Harness, Iap2OutboundObserver, MockWsClient};

/// Drives a gateway companion (announce / authority / now-playing). Uniform
/// across tiers: T1 in-process duplex, T3 real radio over rfcomm or iAP2 EA.
#[async_trait]
pub trait GatewayDriver {
  async fn gateway(&self) -> Result<Gateway>;
}

/// Drives the iAP2 control session as the iPhone media source. T1/T2 inject
/// `SessionEvent`s; T3 drives the device-half emulator over the real radio.
#[async_trait]
pub trait Iap2SourceDriver {
  type Source: Iap2Source;
  async fn iap2_source(&self) -> Result<Self::Source>;
}

/// The push surface a scenario drives once it holds an iAP2 source.
#[async_trait]
pub trait Iap2Source: Send + Sync {
  async fn push_now_playing(&self, update: Iap2NowPlaying) -> Result<()>;
  async fn push_artwork(&self, transfer_id: u8, bytes: Vec<u8>) -> Result<()>;
}

/// Opens a modern-mode client, the live recipient a now-playing broadcast
/// egresses to (and what the frame-tap observes).
#[async_trait]
pub trait ModernClientDriver {
  async fn modern_client(&self) -> Result<MockWsClient>;
}

/// Observes the daemon's egress frame stream, the portable assertion surface
/// available at every tier.
#[async_trait]
pub trait FrameObserve {
  async fn frames(&self) -> Result<FrameObserver>;
}

/// Drives webapp commands (player / net / asset / config / webapp / geo)
/// through the real client SDK, the same surface an on-device webapp uses.
#[async_trait]
pub trait CommandDriver {
  async fn command_client(&self) -> Result<CommandClient>;
}

/// Observes outbound iAP2 transport commands (HID pulses, SetNPI). Headless
/// only: the tap is fed by the in-process coordinator's drain of the transport
/// channel, so scenarios binding this lift to T1 (not the over-air tiers,
/// where the live session consumes the channel and the iPhone is the observer).
#[async_trait]
pub trait Iap2OutboundObserve {
  async fn iap2_outbound(&self) -> Result<Iap2OutboundObserver>;
}

#[async_trait]
impl GatewayDriver for Harness {
  async fn gateway(&self) -> Result<Gateway> {
    self.connect_android().await
  }
}

#[async_trait]
impl FrameObserve for Harness {
  async fn frames(&self) -> Result<FrameObserver> {
    Ok(self.observe_frames())
  }
}

#[async_trait]
impl ModernClientDriver for Harness {
  async fn modern_client(&self) -> Result<MockWsClient> {
    self.connect_modern_client().await
  }
}

#[async_trait]
impl CommandDriver for Harness {
  async fn command_client(&self) -> Result<CommandClient> {
    self.connect_command_client().await
  }
}

#[async_trait]
impl Iap2OutboundObserve for Harness {
  async fn iap2_outbound(&self) -> Result<Iap2OutboundObserver> {
    Ok(self.observe_iap2_outbound())
  }
}

#[async_trait]
impl Iap2SourceDriver for Harness {
  type Source = HarnessIap2Source;
  async fn iap2_source(&self) -> Result<HarnessIap2Source> {
    Ok(HarnessIap2Source {
      iap2: self.inject.iap2.clone(),
      addr: self.iap2_peer(),
    })
  }
}

/// T1/T2 iAP2 source: injects `SessionEvent`s on a fixed peer address, the
/// same channel the real `Iap2Manager` feeds.
pub struct HarnessIap2Source {
  iap2: Iap2InjectTx,
  addr: Address,
}

#[async_trait]
impl Iap2Source for HarnessIap2Source {
  async fn push_now_playing(&self, update: Iap2NowPlaying) -> Result<()> {
    self
      .iap2
      .send(Iap2Event {
        address: self.addr,
        event: SessionEvent::NowPlayingUpdate(update),
      })
      .await
      .map_err(|_| anyhow::anyhow!("iap2 inject channel closed"))
  }

  async fn push_artwork(&self, transfer_id: u8, bytes: Vec<u8>) -> Result<()> {
    self
      .iap2
      .send(Iap2Event {
        address: self.addr,
        event: SessionEvent::ArtworkBytes {
          transfer_id,
          bytes: bytes.into(),
        },
      })
      .await
      .map_err(|_| anyhow::anyhow!("iap2 inject channel closed"))
  }
}

#[async_trait]
impl FrameObserve for DeviceHarness {
  async fn frames(&self) -> Result<FrameObserver> {
    self.frame_tap().await
  }
}

#[async_trait]
impl ModernClientDriver for DeviceHarness {
  async fn modern_client(&self) -> Result<MockWsClient> {
    self.connect_modern_client().await
  }
}

#[async_trait]
impl Iap2SourceDriver for DeviceHarness {
  type Source = DeviceIap2Source;
  async fn iap2_source(&self) -> Result<DeviceIap2Source> {
    Ok(DeviceIap2Source {
      handle: self.connect_iap2_emulator().await?,
    })
  }
}

/// T3 iAP2 source: drives the device-half emulator's control session over the
/// real radio via its runtime handle.
pub struct DeviceIap2Source {
  handle: DeviceEmulatorHandle,
}

#[async_trait]
impl Iap2Source for DeviceIap2Source {
  async fn push_now_playing(&self, update: Iap2NowPlaying) -> Result<()> {
    self.handle.push_now_playing(update).await?;
    Ok(())
  }

  async fn push_artwork(&self, transfer_id: u8, bytes: Vec<u8>) -> Result<()> {
    self.handle.push_artwork(transfer_id, bytes.into()).await?;
    Ok(())
  }
}

/// Which over-air transport a T3 gateway scenario uses. The same `Gateway`
/// rides both, so one scenario body lifts across them.
#[derive(Clone, Copy)]
pub enum OverAirTransport {
  Rfcomm,
  Iap2Ea,
}

/// A T3 tier bound to one over-air gateway transport. Wraps a [`DeviceHarness`]
/// (a type cannot implement [`GatewayDriver`] twice) and delegates the observer
/// and client capabilities to it.
pub struct DeviceTier {
  harness: DeviceHarness,
  transport: OverAirTransport,
}

impl DeviceTier {
  pub fn new(harness: DeviceHarness, transport: OverAirTransport) -> Self {
    Self { harness, transport }
  }
}

#[async_trait]
impl GatewayDriver for DeviceTier {
  async fn gateway(&self) -> Result<Gateway> {
    match self.transport {
      OverAirTransport::Rfcomm => self.harness.connect_over_air().await,
      OverAirTransport::Iap2Ea => self.harness.connect_over_air_iap2().await,
    }
  }
}

#[async_trait]
impl FrameObserve for DeviceTier {
  async fn frames(&self) -> Result<FrameObserver> {
    self.harness.frame_tap().await
  }
}

#[async_trait]
impl ModernClientDriver for DeviceTier {
  async fn modern_client(&self) -> Result<MockWsClient> {
    self.harness.connect_modern_client().await
  }
}
