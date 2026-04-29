use std::collections::HashMap;

use bluer::{
  Adapter, AdapterEvent, Session,
  rfcomm::{SocketAddr, Stream},
};
use futures::StreamExt;
use libbridgething::{BRIDGETHING_DEVICE_CLASS, BRIDGETHING_RFCOMM_CHANNEL};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

mod auth;
#[cfg(debug_assertions)]
mod debug;

use crate::{AdapterDevice, Callbacks, JsMessage, MsgRx, Result, bdaddr::BDAddr, protocol::Protocol};

/// Internal connection-side event surfaced to the adapter event loop. The
/// loop fans these out as JS-visible `AdapterEvent::Bytes` /
/// `AdapterEvent::Disconnected` calls.
#[derive(Debug)]
enum ConnectionEvent {
  Bytes(Vec<u8>),
  Closed,
}

type ConnectionTx = tokio::sync::mpsc::Sender<(BDAddr, ConnectionEvent)>;
type ConnectionRx = tokio::sync::mpsc::Receiver<(BDAddr, ConnectionEvent)>;
type WriteTx = tokio::sync::mpsc::Sender<Vec<u8>>;
type WriteRx = tokio::sync::mpsc::Receiver<Vec<u8>>;

/// Per-peer state: a write channel feeding the writer task, plus the read
/// loop join handle.
#[derive(Debug)]
struct Connection {
  write_tx: WriteTx,
  _reader: JoinHandle<()>,
  _writer: JoinHandle<()>,
}

impl Connection {
  fn spawn(address: BDAddr, stream: Stream, conn_tx: ConnectionTx, cancel_token: CancellationToken) -> Self {
    let (read_half, write_half) = tokio::io::split(stream);
    let (write_tx, write_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(16);

    let _reader = tokio::spawn(reader_task(address, read_half, conn_tx.clone(), cancel_token.clone()));
    let _writer = tokio::spawn(writer_task(address, write_half, write_rx, cancel_token));

    Self {
      write_tx,
      _reader,
      _writer,
    }
  }
}

async fn reader_task(
  address: BDAddr,
  mut read_half: tokio::io::ReadHalf<Stream>,
  tx: ConnectionTx,
  cancel_token: CancellationToken,
) {
  let mut buf = vec![0u8; 4096];
  loop {
    tokio::select! {
      result = read_half.read(&mut buf) => match result {
        Ok(0) => {
          tracing::debug!("({address}) rfcomm read returned EOF");
          break;
        }
        Ok(n) => {
          if let Err(e) = tx.send((address, ConnectionEvent::Bytes(buf[..n].to_vec()))).await {
            tracing::error!("({address}) failed to forward rfcomm chunk: {:?}", e);
            break;
          }
        }
        Err(e) => {
          tracing::debug!("({address}) rfcomm read error: {:?}", e);
          break;
        }
      },
      _ = cancel_token.cancelled() => break,
    }
  }

  tracing::info!("({address}) rfcomm reader exited");
  if let Err(e) = tx.send((address, ConnectionEvent::Closed)).await {
    tracing::error!("({address}) failed to send close event: {:?}", e);
  }
}

async fn writer_task(
  address: BDAddr,
  mut write_half: tokio::io::WriteHalf<Stream>,
  mut rx: WriteRx,
  cancel_token: CancellationToken,
) {
  loop {
    tokio::select! {
      Some(frame) = rx.recv() => {
        if let Err(e) = write_half.write_all(&frame).await {
          tracing::error!("({address}) rfcomm write failed: {:?}", e);
          break;
        }
        if let Err(e) = write_half.flush().await {
          tracing::error!("({address}) rfcomm flush failed: {:?}", e);
          break;
        }
      }
      _ = cancel_token.cancelled() => break,
      else => break,
    }
  }
  let _ = write_half.shutdown().await;
}

pub struct Rfcomm {
  adapter: Adapter,
  _agent: bluer::agent::AgentHandle,

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
        Some((addr, evt)) = self.conn_rx.recv() => {
          if let Err(e) = self.handle_connection_event(addr, evt).await {
            tracing::error!("failed to handle connection event: {:?}", e);
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
      JsMessage::Send(addr, frame) => self.send(addr, frame).await?,
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

    let connection = Connection::spawn(
      device.address().into(),
      stream,
      self.conn_tx.clone(),
      self.cancel_token.child_token(),
    );
    self.connections.insert(address, connection);
    tracing::debug!("rfcomm connection established with device: {address}");

    self.stop_discovery().await?;

    let name = device.name().await.unwrap_or_default().unwrap_or_default();
    self.callbacks.send(crate::AdapterEvent::Connected {
      device: AdapterDevice {
        id: device.address().to_string(),
        name,
      },
    });

    Ok(())
  }

  async fn send(&mut self, addr: BDAddr, frame: Vec<u8>) -> Result<()> {
    tracing::trace!("rfcomm send to {addr}: {} bytes", frame.len());

    let bluer_addr: bluer::Address = addr.into();
    if let Some(connection) = self.connections.get(&bluer_addr) {
      if let Err(e) = connection.write_tx.send(frame).await {
        tracing::warn!("write channel closed for {addr}: {:?}", e);
      }
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

  async fn handle_connection_event(&mut self, addr: BDAddr, evt: ConnectionEvent) -> Result<()> {
    tracing::trace!("received connection event from {addr}: {evt:?}");

    match evt {
      ConnectionEvent::Bytes(data) => {
        self.callbacks.send(crate::AdapterEvent::Bytes {
          device_id: addr.to_string(),
          data,
        });
      }
      ConnectionEvent::Closed => {
        if let Err(e) = self.handle_disconnect(addr).await {
          tracing::error!("failed to handle disconnect: {:?}", e);
        }
        self.callbacks.send(crate::AdapterEvent::Disconnected {
          device_id: addr.to_string(),
        });
      }
    }
    Ok(())
  }

  async fn handle_disconnect(&mut self, addr: BDAddr) -> Result<()> {
    tracing::debug!("rfcomm connection closed for {addr}");

    let bluer_addr: bluer::Address = addr.into();
    if self.connections.remove(&bluer_addr).is_none() {
      tracing::trace!("no connection found for addr: {addr}?");
      return Ok(());
    }
    tracing::debug!("removed connection for addr: {addr}");

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
    let bluer_addr: bluer::Address = addr.into();
    self.connections.remove(&bluer_addr);
    // Reader task drops shortly after the stream is closed; the surrounding
    // ConnectionEvent::Closed propagates a Disconnected event to JS.
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
