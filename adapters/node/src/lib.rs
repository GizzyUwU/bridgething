use std::sync::Arc;

use napi::{bindgen_prelude::*, threadsafe_function::ThreadsafeFunction};

#[macro_use]
extern crate napi_derive;

mod adapter;
mod bdaddr;
mod monitoring;
mod protocol;

#[cfg(feature = "ble")]
mod ble;
#[cfg(feature = "rfcomm")]
mod rfcomm;

use bdaddr::BDAddr;

type Event = AdapterEvent;
type Callback = ThreadsafeFunction<Event, Unknown, Event, false>;

type MsgTx = tokio::sync::mpsc::Sender<JsMessage>;
type MsgRx = tokio::sync::mpsc::Receiver<JsMessage>;

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

pub const ADAPTER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[napi]
pub fn adapter_version() -> String {
  format!("v{}", ADAPTER_VERSION)
}

#[derive(Clone)]
pub enum JsMessage {
  ScanOn,
  ScanOff,

  Data(BDAddr, Vec<u8>),

  Disconnect(BDAddr),

  Callback(Arc<Callback>),
}

impl std::fmt::Debug for JsMessage {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::ScanOn => write!(f, "JsMessage::ScanOn"),
      Self::ScanOff => write!(f, "JsMessage::ScanOff"),
      Self::Data(addr, data) => f.debug_tuple("JsMessage::Data").field(addr).field(data).finish(),
      Self::Disconnect(addr) => f.debug_tuple("JsMessage::Disconnect").field(addr).finish(),
      Self::Callback(_) => write!(f, "JsMessage::Callback(...)"),
    }
  }
}

#[napi(string_enum)]
#[derive(Debug, Clone, Copy)]
pub enum ConnectionType {
  Ble,
  Rfcomm,
}

#[napi]
pub enum AdapterEvent {
  Connected {
    name: String,
    device_id: String,
    mode: ConnectionType,
  },
  Disconnected {
    device_id: String,
    mode: ConnectionType,
  },

  Data {
    device_id: String,
    data: Uint8Array,
  },
}

impl Clone for AdapterEvent {
  fn clone(&self) -> Self {
    match self {
      AdapterEvent::Data { device_id, data } => AdapterEvent::Data {
        device_id: device_id.clone(),
        data: Uint8Array::with_data_copied(data),
      },
      this => this.clone(),
    }
  }
}

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error(transparent)]
  NApi(#[from] napi::Error),
  #[cfg(feature = "ble")]
  #[error(transparent)]
  Btleplug(#[from] btleplug::Error),
  #[error("adapter not initialized")]
  NotInitialized,
  #[error("adapter already initialized")]
  AlreadyInitialized,
  #[error("no bluetooth adapters found")]
  NoBluetoothAdapters,
  #[error("adapter not found")]
  BluetoothAdapterNotFound,
  #[error("could not find bridgething characteristic on device")]
  NoCharacteristic,
  #[error("error communicating with the bluetooth thread")]
  Communication(#[from] tokio::sync::mpsc::error::SendError<JsMessage>),
  #[error("error communicating with the bluetooth thread")]
  TryCommunication(#[from] tokio::sync::mpsc::error::TrySendError<JsMessage>),
  #[error("error communicating with the device thread")]
  InternalRecvCommunication(#[from] tokio::sync::mpsc::error::SendError<protocol::NotifyData>),
  #[error("error communicating with the device thread")]
  InternalSendCommunication(#[from] tokio::sync::mpsc::error::SendError<Vec<u8>>),
  #[error("irrecoverable io error")]
  Io(#[from] std::io::Error),
  #[error("device is not connected!")]
  DeviceDisconnected,
  #[cfg(target_os = "linux")]
  #[error("bluez error: {0}")]
  Bluez(#[from] bluer::Error),
}

impl From<Error> for napi::Error {
  fn from(error: Error) -> Self {
    napi::Error::from_reason(error.to_string())
  }
}

impl From<Callback> for JsMessage {
  fn from(callback: Callback) -> Self {
    Self::Callback(Arc::new(callback))
  }
}
