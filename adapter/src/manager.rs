use std::{collections::HashMap, io::Write};

use btleplug::{
  api::{
    bleuuid::BleUuid, BDAddr, Central, CentralEvent, Characteristic, Manager as _, Peripheral as _,
    PeripheralProperties, ScanFilter, ValueNotification,
  },
  platform::{Manager, Peripheral, PeripheralId},
};

use futures::StreamExt;
use libbridgething::{BRIDGETHING_CHARACTERISTIC_UUID, BRIDGETHING_SERVICE_UUID};
use napi::{bindgen_prelude::*, threadsafe_function::ThreadsafeFunctionCallMode};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::{AdapterEvent, Callback, Error, Event, JsMessage, MsgRx, Result};

type NotifyTx = tokio::sync::mpsc::Sender<NotifyData>;
type NotifyRx = tokio::sync::mpsc::Receiver<NotifyData>;

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
          self.emit(AdapterEvent::Data { mac_address: msg.address.to_string(), data: msg.data.into() });
        },
        Some(msg) = self.msg_rx.recv() => {
          if let Err(err) = self.handle_message(msg).await {
            tracing::error!("error handling ble event: {:?}", err);
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
        let services: Vec<String> = services.into_iter().map(|s| s.to_short_string()).collect();
        tracing::debug!("ServicesAdvertisement: {:?}, {:?}", id, services);
      }
      _ => {}
    }

    Ok(())
  }

  async fn handle_device_discovered(&mut self, id: PeripheralId) -> Result<()> {
    let handle = self.adapter.peripheral(&id).await?;

    let properties = handle.properties().await?;
    if let Some(properties) = properties {
      if !properties.services.contains(&BRIDGETHING_SERVICE_UUID) {
        // tracing::trace!("device with mac {:?} did not have bridgething service", handle.address());
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
    let handle = self.adapter.peripheral(&id).await?;
    let device = Device::new(handle, self.notify_tx.clone(), self.cancel_token.child_token()).await?;

    tracing::debug!("device with mac {:?} connected", device.address);
    self.emit(AdapterEvent::Connected {
      name: device.name.clone(),
      mac_address: device.address.to_string(),
    });

    self.devices.insert(device.address, device);
    Ok(())
  }

  async fn handle_disconnect(&mut self, id: PeripheralId) -> Result<()> {
    let handle = self.adapter.peripheral(&id).await?;
    tracing::debug!("device with mac {:?} disconnected", handle.address());

    self.emit(AdapterEvent::Disconnected {
      mac_address: handle.address().to_string(),
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
      JsMessage::Data(address, data) => tracing::debug!("sending data {:?} to {:?}", address, data),
      JsMessage::Disconnect(address) => self.disconnect(address).await?,
      JsMessage::Callback(callback) => {
        tracing::debug!("new callback registered");
        self.callbacks.push(callback);
      }
    }

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

  pub char: Characteristic,

  handle: Peripheral,
  _notify_handle: JoinHandle<Result<()>>,
}

impl Device {
  pub async fn new(handle: Peripheral, tx: NotifyTx, cancel_token: CancellationToken) -> Result<Self> {
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

    // handle
    //   .write(&char, &[0xf, 0x0, 0x0, 0xd], WriteType::WithoutResponse)
    //   .await?;

    Ok(Self {
      name: device_name(&properties),
      address: handle.address(),

      char,

      _notify_handle: DeviceNotify::spawn(handle.address(), notify_stream, tx, cancel_token),
      handle,
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
      mac_address: notification.address.to_string(),
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

  async fn handle_data(&mut self, data: ValueNotification) -> Result<()> {
    tracing::trace!("received from {:?} data: {:?}", self.address, data);

    let mut decoder = flate2::write::GzDecoder::new(Vec::new());
    decoder.write_all(&data.value)?;
    let decoded = decoder.finish()?;
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
