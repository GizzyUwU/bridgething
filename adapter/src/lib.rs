use btleplug::api::BDAddr;
use napi::{bindgen_prelude::*, threadsafe_function::ThreadsafeFunction};

#[macro_use]
extern crate napi_derive;

mod adapter;
mod manager;
mod monitoring;

type Event = AdapterEvent;
type Callback = ThreadsafeFunction<Event, Unknown, Event, false>;
type MsgTx = tokio::sync::mpsc::Sender<JsMessage>;
type MsgRx = tokio::sync::mpsc::Receiver<JsMessage>;

pub enum JsMessage {
  ScanOn,
  ScanOff,

  Data(BDAddr, Vec<u8>),

  Disconnect(BDAddr),

  Callback(Callback),
}

#[napi]
#[derive(Clone)]
pub enum AdapterEvent {
  Connected { name: String, mac_address: String },
  Disconnected { mac_address: String },

  Data { mac_address: String, data: Uint8Array },
}

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error(transparent)]
  NApi(#[from] napi::Error),
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
  #[error("error communicating with the device thread")]
  InternalCommunication(#[from] tokio::sync::mpsc::error::SendError<manager::NotifyData>),
  #[error("irrecoverable io error")]
  Io(#[from] std::io::Error),
}

impl From<Error> for napi::Error {
  fn from(error: Error) -> Self {
    napi::Error::from_reason(error.to_string())
  }
}

impl From<Callback> for JsMessage {
  fn from(callback: Callback) -> Self {
    Self::Callback(callback)
  }
}
