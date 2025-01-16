use std::{collections::BTreeMap, pin::Pin};

use bluer::{
  adv::{Advertisement, AdvertisementHandle},
  gatt::{
    local::{
      characteristic_control, service_control, Application, ApplicationHandle, Characteristic, CharacteristicControl,
      CharacteristicControlEvent, CharacteristicNotify, CharacteristicNotifyMethod, CharacteristicWrite,
      CharacteristicWriteMethod, Service, ServiceControl,
    },
    CharacteristicReader, CharacteristicWriter,
  },
  Adapter,
};
use futures::{future, StreamExt};
use libbridgething::{BRIDGETHING_CHARACTERISTIC_UUID, BRIDGETHING_SERVICE_UUID, MANUFACTURER_ID};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  task::JoinHandle,
};

use super::{BluetoothResult, BluetoothTx};

pub struct GattServer {
  tx: BluetoothTx,
  adapter: Adapter,

  app: ApplicationHandle,
  advertisement: AdvertisementHandle,
  characteristic: Pin<Box<CharacteristicControl>>,
  service: Pin<Box<ServiceControl>>,

  reader: Option<CharacteristicReader>,
  writer: Option<CharacteristicWriter>,

  read_buf: Vec<u8>,
}

impl GattServer {
  pub async fn init(adapter: Adapter, tx: BluetoothTx) -> BluetoothResult<Self> {
    tracing::debug!(
      "advertising on bluetooth adapter {} with address {}",
      adapter.name(),
      adapter.address().await?
    );

    let mut manufacturer_data = BTreeMap::new();
    manufacturer_data.insert(MANUFACTURER_ID, vec![0x21, 0x22, 0x23, 0x24]);
    let le_advertisement = Advertisement {
      service_uuids: vec![BRIDGETHING_SERVICE_UUID].into_iter().collect(),
      manufacturer_data,
      discoverable: Some(true),
      local_name: Some("bridgething".to_string()),
      ..Default::default()
    };
    let adv_handle = adapter.advertise(le_advertisement).await?;

    tracing::debug!("serving gatt service on bluetooth adapter {}", adapter.name());
    let (service_control, service_handle) = service_control();
    let (char_control, char_handle) = characteristic_control();
    let app = Application {
      services: vec![Service {
        uuid: BRIDGETHING_SERVICE_UUID,
        primary: true,
        characteristics: vec![Characteristic {
          uuid: BRIDGETHING_CHARACTERISTIC_UUID,
          write: Some(CharacteristicWrite {
            write: true,
            write_without_response: true,
            method: CharacteristicWriteMethod::Io,
            ..Default::default()
          }),
          notify: Some(CharacteristicNotify {
            notify: true,
            method: CharacteristicNotifyMethod::Io,
            ..Default::default()
          }),
          control_handle: char_handle,
          ..Default::default()
        }],
        control_handle: service_handle,
        ..Default::default()
      }],
      ..Default::default()
    };
    let app_handle = adapter.serve_gatt_application(app).await?;

    tracing::trace!("service handle is 0x{:x}", service_control.handle()?);
    tracing::trace!("characteristic handle is 0x{:x}", char_control.handle()?);

    Ok(Self {
      tx,
      adapter,

      app: app_handle,
      advertisement: adv_handle,
      characteristic: Box::pin(char_control),
      service: Box::pin(service_control),

      reader: None,
      writer: None,

      read_buf: Vec::new(),
    })
  }

  pub async fn spawn(mut self) -> JoinHandle<BluetoothResult<()>> {
    tokio::spawn(async move { self.recv().await })
  }

  async fn recv(&mut self) -> BluetoothResult<()> {
    loop {
      tokio::select! {
        evt = self.characteristic.next() => {
          match evt {
            Some(CharacteristicControlEvent::Write(req)) => {
              tracing::debug!("accepting write request event with MTU {}", req.mtu());
              self.read_buf = vec![0; req.mtu()];
              self.reader = Some(req.accept()?);
            },
            Some(CharacteristicControlEvent::Notify(notifier)) => {
              tracing::debug!("accepting notify request event with MTU {}", notifier.mtu());
              self.writer = Some(notifier);
            },
            None => break,
          }
        },
        read_res = async {
          match &mut self.reader {
            Some(reader) if self.writer.is_some() => reader.read(&mut self.read_buf).await,
            _ => future::pending().await,
          }
        } => {
          match read_res {
            Ok(0) => {
              tracing::debug!("read stream ended");
              self.reader = None;
            }
            Ok(n) => {
              let value = self.read_buf[..n].to_vec();
              tracing::trace!("echoing {} bytes: {:x?} ... {:x?}", value.len(), &value[0..4.min(value.len())], &value[value.len().saturating_sub(4) ..]);

              let Some(writer) = self.writer.as_mut() else {
                tracing::error!("could not get reference to writer?? this should never fail.");
                continue;
              };

              if let Err(err) = writer.write_all(&value).await {
                tracing::error!("write failed: {}", &err);
                self.writer = None;
              }
            }
            Err(err) => {
              tracing::error!("read stream error: {}", &err);
              self.reader = None;
            }
          }
        }
      }
    }

    Ok(())
  }
}
