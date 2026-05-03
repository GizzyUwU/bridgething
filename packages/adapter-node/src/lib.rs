use std::sync::Arc;

use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode, UnknownReturnValue};

#[macro_use]
extern crate napi_derive;

mod adapter;
mod bdaddr;
mod monitoring;
mod protocol;

#[cfg(feature = "rfcomm")]
mod rfcomm;

use bdaddr::BDAddr;

type Event = AdapterEvent;
type Callback = ThreadsafeFunction<Event, UnknownReturnValue, Event, napi::Status, false>;

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

/// Messages flowing JS → adapter event loop. The wire-level codec lives one
/// layer up in `@bridgething/gateway`; this layer trades raw bytes only.
#[derive(Clone)]
pub enum JsMessage {
  Send(BDAddr, Vec<u8>),
  Disconnect(BDAddr),
  Callback(Arc<Callback>),
}

impl std::fmt::Debug for JsMessage {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Send(addr, data) => f
        .debug_tuple("JsMessage::Send")
        .field(addr)
        .field(&format!("<{} bytes>", data.len()))
        .finish(),
      Self::Disconnect(addr) => f.debug_tuple("JsMessage::Disconnect").field(addr).finish(),
      Self::Callback(_) => write!(f, "JsMessage::Callback(...)"),
    }
  }
}

/// Identity for a connected peer.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct AdapterDevice {
  pub id: String,
  pub name: String,
}

/// Byte-level adapter events. Frame reassembly + msgpack/gzip codec are the
/// gateway layer's responsibility; we ship raw transport chunks unmodified.
///
/// `data` is `Vec<u8>` (not `Buffer`) so the variant stays `Clone` for fan-out
/// across multiple JS listeners. napi-rs marshals it as a `Uint8Array` on the
/// JS side, which the TS adapter shim consumes directly.
#[napi]
#[derive(Clone)]
pub enum AdapterEvent {
  Connected { device: AdapterDevice },
  Disconnected { device_id: String },
  Bytes { device_id: String, data: Vec<u8> },
}

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error(transparent)]
  NApi(#[from] napi::Error),
  #[error("adapter not started")]
  NotInitialized,
  #[error("adapter already started")]
  AlreadyInitialized,
  #[error("no bluetooth adapters found")]
  NoBluetoothAdapters,
  #[error("adapter not found")]
  BluetoothAdapterNotFound,
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
