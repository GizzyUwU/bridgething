use bluer::{
  rfcomm::{Socket, SocketAddr},
  Adapter, AdapterEvent, DiscoveryFilter, DiscoveryTransport, Session,
};
use futures::{Stream, StreamExt};
use libbridgething::{BRIDGETHING_PROFILE_UUID, BRIDGETHING_RFCOMM_CHANNEL};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

#[cfg(debug_assertions)]
mod debug;

use crate::{protocol::Protocol, Callback, JsMessage, MsgRx, Result};

pub struct Rfcomm {
  session: Session,
  adapter: Adapter,

  discovery: Option<Box<dyn Stream<Item = AdapterEvent> + Send + Unpin>>,

  rx: MsgRx,
  callbacks: Vec<Callback>,
  cancel_token: CancellationToken,
}

impl Rfcomm {
  async fn event_loop(&mut self) {
    if let Err(e) = self.initialize().await {
      tracing::error!("failed to initialize rfcomm?? {:?}", e);
    };

    tracing::debug!("starting rfcomm event loop");

    loop {
      tokio::select! {
        Some(msg) = self.rx.recv() => {
          if let Err(e) = self.handle_js_msg(msg).await {
            tracing::error!("failed to handle js message: {:?}", e);
          }
        }
        Some(event) = async {
          if let Some(discovery) = &mut self.discovery {
            discovery.next().await
          } else {
            None
          }
        } => {
          if let Err(e) = self.handle_adapter_event(event).await {
            tracing::error!("failed to handle adapter event: {:?}", e);
          }
        }
        _ = self.cancel_token.cancelled() => {
          tracing::debug!("rfcomm event loop cancelled");
          break;
        }
      }
    }

    tracing::debug!("rfcomm event loop exited");
  }

  async fn handle_js_msg(&mut self, msg: JsMessage) -> Result<()> {
    tracing::trace!("received JS message: {msg:?}");

    Ok(())
  }

  async fn handle_adapter_event(&mut self, event: AdapterEvent) -> Result<()> {
    tracing::trace!("received adapter event: {event:?}");

    let address = match event {
      AdapterEvent::DeviceAdded(address) => {
        tracing::debug!("device added: {address}");
        address
      }
      AdapterEvent::DeviceRemoved(address) => {
        tracing::debug!("device removed: {address}");
        return Ok(());
      }
      AdapterEvent::PropertyChanged(adapter_property) => {
        tracing::debug!("adapter property changed: {adapter_property:?}");
        return Ok(());
      }
    };

    let device = self.adapter.device(address)?;
    if device.address().to_string() != "50:C2:E8:D7:1C:D2" {
      tracing::debug!("skipping device: {address}");
      return Ok(());
    }

    let services_resolved = device.is_services_resolved().await?;
    let paired = device.is_paired().await?;
    let connected = device.is_connected().await?;
    let trusted = device.is_trusted().await?;
    tracing::debug!(
      "services_resolved: {services_resolved}, paired: {paired}, connected: {connected}, trusted: {trusted}"
    );

    #[cfg(debug_assertions)]
    debug::query_device(&device).await?;

    if !paired {
      tracing::debug!("pairing with device: {address}");
      device.pair().await?;
    }

    // attempt to connect rfcomm
    let socket_addr = SocketAddr::new(device.address(), BRIDGETHING_RFCOMM_CHANNEL);
    let socket = Socket::new()?;
    let stream = socket.connect(socket_addr).await?;

    Ok(())
  }

  async fn initialize(&mut self) -> Result<()> {
    tracing::debug!("initializing rfcomm protocol");
    self.start_discovery().await?;

    Ok(())
  }

  async fn start_discovery(&mut self) -> Result<()> {
    tracing::debug!("discovering devices");

    let filter = DiscoveryFilter {
      uuids: [BRIDGETHING_PROFILE_UUID].into(),
      transport: DiscoveryTransport::BrEdr,
      duplicate_data: false,
      ..Default::default()
    };
    self.adapter.set_discovery_filter(filter).await?;

    let devices = self.adapter.discover_devices_with_changes().await?;
    self.discovery = Some(Box::new(devices));

    Ok(())
  }
}

#[async_trait::async_trait]
impl Protocol for Rfcomm {
  async fn init(
    adapter_name: Option<String>,
    rx: MsgRx,
    callbacks: Vec<Callback>,
    cancel_token: CancellationToken,
  ) -> Result<Self> {
    tracing::debug!("initializing bluetooth adapter");
    let session = Session::new().await?;
    let adapter = if let Some(adapter_name) = adapter_name {
      session.adapter(&adapter_name)
    } else {
      session.default_adapter().await
    }?;

    tracing::debug!("attempting to power on adapter");
    adapter.set_powered(true).await?;
    adapter.set_pairable(true).await?;

    #[cfg(debug_assertions)]
    debug::query_adapter(&adapter).await?;

    tracing::debug!("initialized bluetooth adapter {}", adapter.name());

    Ok(Self {
      session,
      adapter,

      discovery: None,

      rx,
      callbacks,
      cancel_token,
    })
  }

  fn spawn(mut self) -> JoinHandle<()> {
    tokio::spawn(async move { self.event_loop().await })
  }
}
