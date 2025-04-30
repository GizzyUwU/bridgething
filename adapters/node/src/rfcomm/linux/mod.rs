use std::collections::HashMap;

use bluer::{
  agent::AgentHandle,
  rfcomm::{SocketAddr, Stream},
  Adapter, AdapterEvent, Session,
};
use futures::{
  stream::{SplitSink, SplitStream},
  SinkExt, StreamExt,
};
use libbridgething::{
  gateway::GatewayToBridgeMsg, protocol::GatewayEndec, BRIDGETHING_DEVICE_CLASS, BRIDGETHING_RFCOMM_CHANNEL,
};
use tokio::task::JoinHandle;
use tokio_util::{codec::Framed, sync::CancellationToken};

mod auth;
#[cfg(debug_assertions)]
mod debug;

use crate::{
  bdaddr::BDAddr, protocol::Protocol, Callbacks, ConnectionMessage, ConnectionType, JsMessage, MsgRx, Result,
};

type ConnectionTx = tokio::sync::mpsc::Sender<(BDAddr, ConnectionMessage)>;
type ConnectionRx = tokio::sync::mpsc::Receiver<(BDAddr, ConnectionMessage)>;

#[derive(Debug)]
struct Connection {
  writer: SplitSink<Framed<Stream, GatewayEndec>, GatewayToBridgeMsg>,
  _reader_handle: JoinHandle<()>,
}

impl Connection {
  fn new(address: BDAddr, stream: Stream, tx: ConnectionTx) -> Self {
    let framed = Framed::new(stream, GatewayEndec::default());
    let (writer, reader) = framed.split();
    let _reader_handle = tokio::spawn(reader_task(address, reader, tx));
    Self { writer, _reader_handle }
  }
}

async fn reader_task(address: BDAddr, mut reader: SplitStream<Framed<Stream, GatewayEndec>>, tx: ConnectionTx) {
  while let Some(frame) = reader.next().await {
    match frame {
      Ok(msg) => {
        if let Err(e) = tx.send((address, msg.into())).await {
          tracing::error!("failed to forward bridge message: {:?}", e);
        }
      }
      Err(e) => {
        tracing::debug!("error decoding rfcomm frame: {:?}", e);
        break;
      }
    }
  }

  tracing::info!("({address}) bluetooth connection closed");
  if let Err(e) = tx
    .send((address, ConnectionMessage::Close(ConnectionType::Rfcomm)))
    .await
  {
    tracing::error!("({address}) failed to send close message: {:?}", e);
  }
}

pub struct Rfcomm {
  adapter: Adapter,
  _agent: AgentHandle,

  discovery: Option<Box<dyn futures::Stream<Item = AdapterEvent> + Send + Unpin>>,
  connections: HashMap<bluer::Address, Connection>,

  conn_tx: ConnectionTx,
  conn_rx: ConnectionRx,

  rx: MsgRx,
  callbacks: Callbacks,
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
        Some(msg) = self.conn_rx.recv() => {
          if let Err(e) = self.handle_message(msg.0, msg.1).await {
            tracing::error!("failed to handle disconnect: {:?}", e);
          }
        },
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

    match msg {
      JsMessage::ScanOn => self.start_discovery().await?,
      JsMessage::ScanOff => self.stop_discovery().await?,
      JsMessage::Data(addr, msg) => self.send(addr, msg).await?,
      JsMessage::Disconnect(addr) => self.disconnect(addr).await?,
      JsMessage::Callback(callback) => {
        tracing::debug!("adding new callback");
        self.callbacks.add(callback);
      }
    }

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

    #[cfg(debug_assertions)]
    debug::query_device(&device).await?;

    let paired = device.is_paired().await?;
    let connected = device.is_connected().await?;
    let trusted = device.is_trusted().await?;
    tracing::debug!("({address}) paired: {paired}, connected: {connected}, trusted: {trusted}");

    // work around bluez not letting me sdp easily
    if let Some(class) = device.class().await? {
      tracing::debug!("device class: {class}");
      if class != BRIDGETHING_DEVICE_CLASS {
        tracing::debug!("device class does not match: {address}");
        return Ok(());
      }
    } else {
      tracing::warn!("device class not available");
    }

    #[cfg(debug_assertions)]
    debug::query_device(&device).await?;

    if !trusted {
      tracing::debug!("trusting device: {address}");
      device.set_trusted(true).await?;
    }

    if !paired {
      tracing::debug!("pairing with device: {address}");
      device.pair().await?;
    }

    // attempt to connect rfcomm
    let socket_addr = SocketAddr::new(device.address(), BRIDGETHING_RFCOMM_CHANNEL);
    let stream = Stream::connect(socket_addr).await?;
    tracing::debug!("rfcomm stream connected to device: {address}");

    let connection = Connection::new(device.address().into(), stream, self.conn_tx.clone());
    self.connections.insert(address, connection);
    tracing::debug!("rfcomm connection established with device: {address}");

    self.stop_discovery().await?;

    self.callbacks.send(crate::AdapterEvent::Connected {
      name: device.name().await.unwrap_or_default().unwrap_or_default(),
      device_id: device.address().to_string(),
      mode: crate::ConnectionType::Rfcomm,
    });

    Ok(())
  }

  async fn send(&mut self, addr: BDAddr, msg: GatewayToBridgeMsg) -> Result<()> {
    tracing::trace!("sending rfcomm message to {addr}: {msg:?}");

    if let Some(connection) = self.connections.get_mut(&addr.into()) {
      connection.writer.send(msg).await?;
    } else {
      tracing::warn!("no connection found for address: {addr}");
    }

    Ok(())
  }

  async fn initialize(&mut self) -> Result<()> {
    tracing::debug!("initializing rfcomm protocol");
    self.start_discovery().await?;

    Ok(())
  }

  async fn handle_message(&mut self, addr: BDAddr, msg: ConnectionMessage) -> Result<()> {
    tracing::trace!("received message from {addr}: {msg:?}");

    if matches!(msg, ConnectionMessage::Close(_)) {
      if let Err(e) = self.handle_disconnect(addr).await {
        tracing::error!("failed to handle disconnect: {:?}", e);
      }
    }

    self.callbacks.send((addr, msg).into());
    Ok(())
  }

  async fn handle_disconnect(&mut self, addr: BDAddr) -> Result<()> {
    tracing::debug!("rfcomm connection closed for {addr}");

    let Some(mut connection) = self.connections.remove(&addr.into()) else {
      tracing::trace!("no connection found for addr: {addr}?");
      return Ok(());
    };
    tracing::debug!("removed connection for addr: {addr}");
    let _ = connection.writer.close().await;

    if self.connections.is_empty() {
      tracing::debug!("no more connections, starting discovery");
      self.start_discovery().await?;
    }

    Ok(())
  }

  async fn start_discovery(&mut self) -> Result<()> {
    tracing::debug!("discovering devices");
    if self.discovery.is_some() {
      tracing::debug!("discovery already started");
      return Ok(());
    }

    // let filter = DiscoveryFilter {
    //   uuids: [BRIDGETHING_PROFILE_UUID].into(),
    //   transport: DiscoveryTransport::BrEdr,
    //   duplicate_data: false,
    //   ..Default::default()
    // };
    // self.adapter.set_discovery_filter(filter).await?;

    let devices = self.adapter.discover_devices_with_changes().await?;
    self.discovery = Some(Box::new(devices));

    Ok(())
  }

  async fn stop_discovery(&mut self) -> Result<()> {
    tracing::debug!("stopping discovery");
    if self.discovery.is_some() {
      self.discovery = None;
    }

    Ok(())
  }

  async fn disconnect(&mut self, addr: BDAddr) -> Result<()> {
    tracing::debug!("disconnecting from device: {addr}");
    if let Some(mut connection) = self.connections.remove(&addr.into()) {
      connection.writer.close().await?;
    }

    Ok(())
  }
}

#[async_trait::async_trait]
impl Protocol for Rfcomm {
  async fn init(
    adapter_name: Option<String>,
    rx: MsgRx,
    callbacks: Callbacks,
    cancel_token: CancellationToken,
  ) -> Result<Self> {
    tracing::debug!("initializing bluetooth adapter");
    let session = Session::new().await?;
    let _agent = auth::build_agent(&session).await?;
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

    let (conn_tx, conn_rx) = tokio::sync::mpsc::channel(16);

    Ok(Self {
      adapter,
      _agent,

      discovery: None,
      connections: HashMap::new(),

      conn_tx,
      conn_rx,

      rx,
      callbacks,
      cancel_token,
    })
  }

  fn spawn(mut self) -> JoinHandle<()> {
    tokio::spawn(async move { self.event_loop().await })
  }
}

impl From<bluer::Address> for BDAddr {
  fn from(address: bluer::Address) -> Self {
    let mut addr = [0; 6];
    addr.copy_from_slice(address.as_slice());
    Self::from(addr)
  }
}

impl From<BDAddr> for bluer::Address {
  fn from(address: BDAddr) -> Self {
    let mut addr = [0; 6];
    addr.copy_from_slice(address.as_ref());
    Self::new(addr)
  }
}
