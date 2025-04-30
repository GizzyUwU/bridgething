use std::{collections::BTreeMap, io::Write, pin::Pin};

use bluer::{
  Adapter,
  adv::{Advertisement, AdvertisementHandle, SecondaryChannel},
  gatt::{
    CharacteristicReader, CharacteristicWriter,
    local::{
      Application, ApplicationHandle, Characteristic, CharacteristicControl, CharacteristicControlEvent,
      CharacteristicNotify, CharacteristicNotifyMethod, CharacteristicWrite, CharacteristicWriteMethod, Service,
      ServiceControl, characteristic_control, service_control,
    },
  },
};
use flate2::Compression;
use futures::{StreamExt, future};
use libbridgething::{
  BRIDGETHING_CHARACTERISTIC_UUID, BRIDGETHING_MANUFACTURER_ID, BRIDGETHING_SERVICE_UUID,
  gateway::{BridgeToGatewayMsg, BridgeToGatewayMsgData, GatewayMsgMeta},
};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  task::JoinHandle,
};

use crate::{bluetooth::BluetoothError, state::State};

use super::{BluetoothResult, GatewayRecvTx, GatewaySendRx};

// TODO: handle errors ffs
pub struct GattServer {
  state: State,

  tx: GatewayRecvTx,
  rx: GatewaySendRx,

  app: ApplicationHandle,
  advertisement: AdvertisementHandle,
  characteristic: Pin<Box<CharacteristicControl>>,
  service: Pin<Box<ServiceControl>>,

  reader: Option<CharacteristicReader>,
  writer: Option<CharacteristicWriter>,

  // testing
  start: tokio::time::Instant,
  packets: usize,
  total_bytes: usize,

  read_buf: Vec<u8>,
}

impl GattServer {
  pub async fn init(adapter: &Adapter, state: State, tx: GatewayRecvTx, rx: GatewaySendRx) -> BluetoothResult<Self> {
    tracing::debug!(
      "advertising on bluetooth adapter {} with address {}",
      adapter.name(),
      adapter.address().await?
    );

    let mut manufacturer_data = BTreeMap::new();
    manufacturer_data.insert(BRIDGETHING_MANUFACTURER_ID, vec![0x21, 0x22, 0x23, 0x24]);
    let le_advertisement = Advertisement {
      service_uuids: vec![BRIDGETHING_SERVICE_UUID].into_iter().collect(),
      manufacturer_data,
      discoverable: Some(true),
      local_name: Some("bridgething".to_string()),
      solicit_uuids: vec![BRIDGETHING_SERVICE_UUID].into_iter().collect(),
      secondary_channel: Some(SecondaryChannel::TwoM),
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
            reliable_write: true,
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
      state,

      tx,
      rx,

      app: app_handle,
      advertisement: adv_handle,
      characteristic: Box::pin(char_control),
      service: Box::pin(service_control),

      reader: None,
      writer: None,

      // testing
      start: tokio::time::Instant::now(),
      packets: 0,
      total_bytes: 0,

      read_buf: Vec::new(),
    })
  }

  pub fn spawn(mut self) -> JoinHandle<()> {
    tokio::spawn(async move {
      if let Err(err) = self.recv().await {
        tracing::error!("gatt server died: {:?}", err);
      }
    })
  }

  async fn recv(&mut self) -> BluetoothResult<()> {
    loop {
      tokio::select! {
        Some(data) = self.rx.recv() => self.write(data.msg).await?,

        evt = self.characteristic.next() => self.handle_characteristic(evt).await?,
        read_res = async {
          match &mut self.reader {
            Some(reader) => reader.read(&mut self.read_buf).await,
            None => future::pending().await,
          }
        } => self.handle_read(read_res).await?,
      }
    }
  }

  async fn write(&mut self, msg: BridgeToGatewayMsg) -> BluetoothResult<()> {
    let Some(writer) = self.writer.as_mut() else {
      tracing::error!("could not get reference to writer. is bluetooth disconnected?");
      return Ok(());
    };
    tracing::trace!("writing message: {:?}", msg);

    let packed = rmp_serde::to_vec(&msg)?;
    tracing::trace!("packed msg: {:?}", &packed);

    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&packed)?;
    let message = encoder.finish()?;
    tracing::trace!("final msg: {:?}", &message);

    if let Err(err) = writer.write_all(&message).await {
      tracing::error!("write failed: {}", &err);
      self.writer = None;
      return Err(err)?;
    }

    Ok(())
  }

  async fn handle_read(&mut self, read_res: Result<usize, std::io::Error>) -> BluetoothResult<()> {
    match read_res {
      Ok(0) => {
        tracing::debug!("read stream ended");
        self.reader = None;
      }
      Ok(n) => {
        let data = self.read_buf[..n].to_vec();
        tracing::trace!("read {} bytes: {:x?}", data.len(), &data);
        self.total_bytes += data.len();
        self.packets += 1;
        tracing::trace!("elapsed: {:?}", self.start.elapsed());
        tracing::trace!("packets: {}", self.packets);
        tracing::trace!("total bytes: {}", self.total_bytes);

        // let mut decoder = flate2::write::GzDecoder::new(Vec::new());
        // decoder.write_all(&data)?;
        // let data = decoder.finish()?;
        // tracing::trace!("uncompressed msg: {:x?}", &data);

        // let message = match rmp_serde::from_slice(&data) {
        //   Ok(message) => message,
        //   Err(err) => {
        //     tracing::error!("failed to decode packed message: {:?}", err);
        //     return Ok(());
        //   }
        // };

        // if let Err(err) = self.tx.send(message).await {
        //   tracing::error!("error forwarding bluetooth message: {:?}", err);
        // }
      }
      Err(err) => {
        tracing::error!("read stream error: {}", &err);
        self.reader = None;
      }
    }

    Ok(())
  }

  async fn handle_characteristic(&mut self, evt: Option<CharacteristicControlEvent>) -> BluetoothResult<()> {
    match evt {
      Some(CharacteristicControlEvent::Write(req)) => {
        tracing::debug!("accepting write request event with MTU {}", req.mtu());
        self.read_buf = vec![0; req.mtu()];
        self.reader = Some(req.accept()?);

        self.start = tokio::time::Instant::now();
        self.packets = 0;
        self.total_bytes = 0;
      }
      Some(CharacteristicControlEvent::Notify(notifier)) => {
        tracing::debug!("accepting notify request event with MTU {}", notifier.mtu());
        self.writer = Some(notifier);

        self
          .write(BridgeToGatewayMsg {
            id: uuid::Uuid::now_v7(),
            meta: GatewayMsgMeta::Event,
            data: BridgeToGatewayMsgData::Version {
              data: self.state.meta.clone().into(),
            },
          })
          .await
          .expect("could not send version!!");
      }
      None => {
        tracing::error!("bluetooth characteristic pipe broken!!");
        return Err(BluetoothError::CharacteristicControl);
      }
    }

    Ok(())
  }
}
