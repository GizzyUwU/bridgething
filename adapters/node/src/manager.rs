use std::{collections::HashMap, io::Write};

use btleplug::{
  api::{
    bleuuid::BleUuid, BDAddr, Central, CentralEvent, Characteristic, Manager as _, Peripheral as _,
    PeripheralProperties, ScanFilter, ValueNotification, WriteType,
  },
  platform::{Manager, Peripheral, PeripheralId},
};

use flate2::Compression;
use futures::StreamExt;
use libbridgething::{BRIDGETHING_CHARACTERISTIC_UUID, BRIDGETHING_SERVICE_UUID};
use napi::{bindgen_prelude::*, threadsafe_function::ThreadsafeFunctionCallMode};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::{AdapterEvent, Callback, Error, Event, JsMessage, MsgRx, Result};

type NotifyTx = tokio::sync::mpsc::Sender<NotifyData>;
type NotifyRx = tokio::sync::mpsc::Receiver<NotifyData>;

type WriteTx = tokio::sync::mpsc::Sender<Vec<u8>>;
type WriteRx = tokio::sync::mpsc::Receiver<Vec<u8>>;

pub struct BtMan {
  _manager: Manager,
  adapter: btleplug::platform::Adapter,

  devices: HashMap<BDAddr, Device>,

  msg_rx: MsgRx,
  callbacks: Vec<Callback>,

  notify_rx: NotifyRx,
  notify_tx: NotifyTx,

  cancel_token: CancellationToken,
}

impl BtMan {
  pub async fn new(
    adapter_name: Option<String>,
    msg_rx: MsgRx,
    callbacks: Vec<Callback>,
    cancel_token: CancellationToken,
  ) -> Result<Self> {
    let manager = Manager::new().await?;
    tracing::debug!("initializing adapter {:?}", adapter_name);
    let adapter_list = manager.adapters().await?;
    if adapter_list.is_empty() {
      return Err(Error::NoBluetoothAdapters);
    }
    tracing::trace!("adapters: {:?}", adapter_list);

    let adapter = if let Some(adapter_name) = adapter_name {
      let mut found_adapter = None;
      for adapter in adapter_list {
        if adapter.adapter_info().await?.contains(&adapter_name) {
          found_adapter = Some(adapter);
          break;
        }
      }
      found_adapter.ok_or(Error::BluetoothAdapterNotFound)?
    } else {
      adapter_list.into_iter().next().ok_or(Error::BluetoothAdapterNotFound)?
    };
    tracing::debug!("adapter info: {:?}", adapter.adapter_info().await?);
    tracing::debug!("adapter state: {:?}", adapter.adapter_state().await?);

    let (notify_tx, notify_rx) = tokio::sync::mpsc::channel(16);

    Ok(Self {
      _manager: manager,
      adapter,

      devices: HashMap::new(),

      msg_rx,
      callbacks,

      notify_tx,
      notify_rx,

      cancel_token,
    })
  }

  async fn event_loop(&mut self) -> Result<()> {
    tracing::debug!("initializing bluetooth manager event loop");

    let mut events = self.adapter.events().await?;

    let scan_filter = ScanFilter {
      services: vec![BRIDGETHING_SERVICE_UUID],
    };
    self.adapter.start_scan(scan_filter).await?;
    tracing::debug!("scanning for bridgething service uuid");

    loop {
      tokio::select! {
        Some(event) = events.next() => {
          if let Err(err) = self.handle_event(event).await {
            tracing::error!("error handling ble event: {:?}", err);
          }
        },
        Some(msg) = self.notify_rx.recv() => {
          tracing::debug!("received new from: {:?}", msg.address);
          self.emit(AdapterEvent::Data { device_id: msg.address.to_string(), data: msg.data.into() });
        },
        Some(msg) = self.msg_rx.recv() => {
          if let Err(err) = self.handle_message(msg).await {
            tracing::error!("error handling javascript message: {:?}", err);
          }
        },
        _ = self.cancel_token.cancelled() => break,
      }
    }

    tracing::debug!("cancellation token called - shutting down bluetooth manager");
    Ok(())
  }

  async fn handle_event(&mut self, event: CentralEvent) -> Result<()> {
    match event {
      CentralEvent::DeviceDiscovered(id) => self.handle_device_discovered(id).await?,
      CentralEvent::StateUpdate(state) => tracing::debug!("adapter state update: {:?}", state),
      CentralEvent::DeviceConnected(id) => self.handle_connect(id).await?,
      CentralEvent::DeviceDisconnected(id) => self.handle_disconnect(id).await?,
      CentralEvent::ManufacturerDataAdvertisement { id, manufacturer_data } => {
        tracing::debug!("ManufacturerDataAdvertisement: {:?}, {:?}", id, manufacturer_data);
      }
      CentralEvent::ServiceDataAdvertisement { id, service_data } => {
        tracing::debug!("ServiceDataAdvertisement: {:?}, {:?}", id, service_data);
      }
      CentralEvent::ServicesAdvertisement { id, services } => {
        tracing::debug!(
          "ServicesAdvertisement: {:?}, {:?}",
          id,
          services.iter().map(|s| s.to_short_string()).collect::<Vec<_>>()
        );

        if services.into_iter().any(|s| s == BRIDGETHING_SERVICE_UUID) {
          self.handle_device_discovered(id).await?;
        };
      }
      _ => {}
    }

    Ok(())
  }

  async fn handle_device_discovered(&mut self, id: PeripheralId) -> Result<()> {
    tracing::debug!("handling device {:?} discovered", id);
    let handle = self.adapter.peripheral(&id).await?;

    let properties = handle.properties().await?;
    if let Some(properties) = properties {
      if !properties.services.contains(&BRIDGETHING_SERVICE_UUID) {
        tracing::trace!(
          "device with mac {:?} did not have bridgething service",
          handle.address()
        );
        return Ok(());
      };
    };

    tracing::debug!("discovered device running bridgething with mac: {:?}", handle.address());
    if !handle.is_connected().await? {
      tracing::debug!("attempting to connect to {:?}", handle.address());
      handle.connect().await?;
    }

    Ok(())
  }

  async fn handle_connect(&mut self, id: PeripheralId) -> Result<()> {
    tracing::debug!("handling device {:?} connected", id);
    let handle = self.adapter.peripheral(&id).await?;
    let device = Device::new(handle, self.notify_tx.clone(), self.cancel_token.child_token()).await?;

    tracing::info!("device with mac {:?} connected", device.address);
    self.emit(AdapterEvent::Connected {
      name: device.name.clone(),
      device_id: device.address.to_string(),
    });

    self.devices.insert(device.address, device);
    Ok(())
  }

  async fn handle_disconnect(&mut self, id: PeripheralId) -> Result<()> {
    tracing::debug!("handling device {:?} disconnected", id);
    let handle = self.adapter.peripheral(&id).await?;
    tracing::info!("device with mac {:?} disconnected", handle.address());

    self.emit(AdapterEvent::Disconnected {
      device_id: handle.address().to_string(),
    });

    self.devices.remove(&handle.address());
    Ok(())
  }

  fn emit(&self, event: Event) {
    for callback in &self.callbacks {
      match callback.call(event.clone(), ThreadsafeFunctionCallMode::NonBlocking) {
        Status::Ok => continue,
        other => tracing::error!("failed to send callback: {:?}", other),
      }
    }
  }

  async fn handle_message(&mut self, msg: JsMessage) -> Result<()> {
    match msg {
      JsMessage::ScanOn => tracing::debug!("received scan on message"),
      JsMessage::ScanOff => tracing::debug!("received scan off message"),
      JsMessage::Data(address, data) => self.handle_send(address, data).await?,
      JsMessage::Disconnect(address) => self.disconnect(address).await?,
      JsMessage::Callback(callback) => {
        tracing::debug!("new callback registered");
        self.callbacks.push(callback);
      }
    }

    Ok(())
  }

  async fn handle_send(&mut self, address: BDAddr, data: Vec<u8>) -> Result<()> {
    tracing::debug!("sending new message to {:?}", address);

    let device = self.devices.get(&address).ok_or(Error::DeviceDisconnected)?;
    device.tx.send(data).await?;

    Ok(())
  }

  async fn disconnect(&mut self, address: BDAddr) -> Result<()> {
    tracing::debug!("disconnecting from device with mac addr {:?}", address);
    let Some(device) = self.devices.get(&address) else {
      tracing::warn!("device with mac address {:?} not known, cannot disconnect", address);
      return Ok(());
    };

    Ok(device.handle.disconnect().await?)
  }

  pub fn spawn(mut self) -> JoinHandle<Result<()>> {
    tokio::spawn(async move { self.event_loop().await })
  }
}

fn device_name(properties: &Option<PeripheralProperties>) -> String {
  properties
    .as_ref()
    .and_then(|p| p.local_name.to_owned())
    .unwrap_or("(device name unknown)".to_string())
}

#[derive(Debug)]
struct Device {
  name: String,
  address: BDAddr,

  tx: WriteTx,
  handle: Peripheral,

  _notify_handle: JoinHandle<Result<()>>,
  _write_handle: JoinHandle<Result<()>>,
}

impl Device {
  pub async fn new(handle: Peripheral, notify_tx: NotifyTx, cancel_token: CancellationToken) -> Result<Self> {
    tracing::debug!("discovering services for {:?}", handle.address());
    handle.discover_services().await?;

    let properties = handle.properties().await?;
    let characteristics = handle.characteristics();
    let char = characteristics
      .into_iter()
      .find(|c| c.uuid == BRIDGETHING_CHARACTERISTIC_UUID)
      .ok_or(crate::Error::NoCharacteristic)?;

    let notify_stream = handle.notifications().await?;
    handle.subscribe(&char).await?;

    let (tx, write_rx) = tokio::sync::mpsc::channel(16);

    Ok(Self {
      name: device_name(&properties),
      address: handle.address(),

      _write_handle: DeviceWrite::spawn(handle.address(), write_rx, char, handle.clone(), cancel_token.clone()),
      _notify_handle: DeviceNotify::spawn(handle.address(), notify_stream, notify_tx, cancel_token),

      tx,
      handle,
    })
  }
}

struct DeviceWrite {
  address: BDAddr,
  rx: WriteRx,

  char: Characteristic,
  handle: Peripheral,

  cancel_token: CancellationToken,
}

impl DeviceWrite {
  async fn event_loop(&mut self) -> Result<()> {
    loop {
      tokio::select! {
        Some(data) = self.rx.recv() => self.handle_write(data).await?,
        _ = self.cancel_token.cancelled() => break,
      }
    }

    Ok(())
  }

  #[tracing::instrument(level = "trace", skip_all)]
  async fn handle_write(&mut self, data: Vec<u8>) -> Result<()> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&data)?;
    let encoded = encoder.finish()?;

    if encoded.len() > 512 {
      tracing::debug!(
        "splitting encoded data of length {} into chunks for {:?}",
        encoded.len(),
        self.address
      );

      for chunk in encoded.chunks(512) {
        // tracing::trace!("writing chunk of length {} to {:?}", chunk.len(), self.address);
        self.handle.write(&self.char, chunk, WriteType::WithoutResponse).await?;
      }
    } else {
      tracing::debug!("writing data of length {} to {:?}", encoded.len(), self.address);
      self
        .handle
        .write(&self.char, &encoded, WriteType::WithoutResponse)
        .await?;
    }

    Ok(())
  }

  pub fn spawn(
    address: BDAddr,
    rx: WriteRx,
    char: Characteristic,
    handle: Peripheral,
    cancel_token: CancellationToken,
  ) -> JoinHandle<Result<()>> {
    tokio::spawn(async move {
      Self {
        address,
        rx,
        char,
        handle,
        cancel_token,
      }
      .event_loop()
      .await
    })
  }
}

#[derive(Debug, Clone)]
pub struct NotifyData {
  address: BDAddr,
  data: Vec<u8>,
}

impl From<NotifyData> for AdapterEvent {
  fn from(notification: NotifyData) -> Self {
    Self::Data {
      device_id: notification.address.to_string(),
      data: notification.data.into(),
    }
  }
}

type NotifyStream = std::pin::Pin<Box<dyn futures::Stream<Item = btleplug::api::ValueNotification> + Send>>;
struct DeviceNotify {
  address: BDAddr,
  tx: NotifyTx,
  stream: NotifyStream,
  cancel_token: CancellationToken,
}

impl DeviceNotify {
  async fn event_loop(&mut self) -> Result<()> {
    loop {
      tokio::select! {
        Some(data) = self.stream.next() => self.handle_data(data).await?,
        _ = self.cancel_token.cancelled() => break,
      }
    }

    Ok(())
  }

  #[tracing::instrument(level = "trace", skip_all)]
  async fn handle_data(&mut self, data: ValueNotification) -> Result<()> {
    tracing::debug!("received from {:?} data of length {:?}", self.address, data.value.len());
    tracing::trace!("received from {:?} data: {:?}", self.address, data);

    let mut decoder = flate2::write::GzDecoder::new(Vec::new());
    decoder.write_all(&data.value)?;
    let decoded = decoder.finish()?;
    tracing::debug!(
      "received from {:?} final message of length {:?}",
      self.address,
      decoded.len()
    );
    tracing::trace!("final msg: {:?}", decoded);

    self
      .tx
      .send(NotifyData {
        address: self.address,
        data: decoded,
      })
      .await?;

    Ok(())
  }

  pub fn spawn(
    address: BDAddr,
    stream: NotifyStream,
    tx: NotifyTx,
    cancel_token: CancellationToken,
  ) -> JoinHandle<Result<()>> {
    tokio::spawn(async move {
      Self {
        address,
        tx,
        stream,
        cancel_token,
      }
      .event_loop()
      .await
    })
  }
}
