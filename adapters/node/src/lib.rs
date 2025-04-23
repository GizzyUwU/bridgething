use std::sync::Arc;

use libbridgething::gateway::{BridgeToGatewayMsg, GatewayToBridgeMsg};
use napi::{
  bindgen_prelude::*,
  threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode},
};

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

#[derive(Default, Clone)]
struct Callbacks(Vec<Arc<Callback>>);

impl Callbacks {
  pub fn add(&mut self, callback: Arc<Callback>) {
    self.0.push(callback);
  }

  pub fn send(&self, event: Event) {
    for callback in &self.0 {
      match callback.call(event.clone(), ThreadsafeFunctionCallMode::Blocking) {
        napi::Status::Ok => {}
        error => {
          tracing::error!("failed to call callback: {:?}", error);
        }
      }
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

  Data(BDAddr, GatewayToBridgeMsg),

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
#[derive(Clone)]
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

  Message {
    device_id: String,
    #[napi(ts_type = "BridgeToGatewayMsg")]
    data: serde_json::Value,
  },
}

impl From<(BDAddr, BridgeToGatewayMsg)> for AdapterEvent {
  fn from((address, msg): (BDAddr, BridgeToGatewayMsg)) -> Self {
    Self::Message {
      device_id: address.to_string(),
      data: serde_json::to_value(msg).unwrap_or_default(),
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
  #[error("irrecoverable io error")]
  Io(#[from] std::io::Error),
  #[error("device is not connected!")]
  DeviceDisconnected,
  #[error("failed to parse address: {0}")]
  AddressParse(#[from] bdaddr::ParseBDAddrError),
  #[error(transparent)]
  Endec(#[from] libbridgething::protocol::EndecError),
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
