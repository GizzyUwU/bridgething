use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

mod bluetooth;
mod setup;
mod storage;

pub use bluetooth::*;
pub use setup::*;
pub use storage::*;

pub type RecvTx = tokio::sync::mpsc::Sender<AddressedRecvMessage>;
pub type RecvRx = tokio::sync::mpsc::Receiver<AddressedRecvMessage>;
pub type SendTx = tokio::sync::mpsc::Sender<SendMessage>;
pub type SendRx = tokio::sync::mpsc::Receiver<SendMessage>;

// --- receives ---
#[derive(Debug)]
pub struct AddressedRecvMessage {
  pub from: SocketAddr,
  pub data: RecvMessageWithMeta,
}

#[derive(Debug)]
pub enum RecvMessageWithMeta {
  Stock(StockRecv),
  ConnectionClosed(tokio_websockets::CloseCode, String),
  Error(tokio_websockets::Error),
}

impl From<RecvMessage> for RecvMessageWithMeta {
  fn from(recv: RecvMessage) -> Self {
    match recv {
      RecvMessage::Stock(msg) => RecvMessageWithMeta::Stock(msg),
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum RecvMessage {
  Stock(StockRecv),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StockRecv {
  Bluetooth(StockBluetoothRecv),
  Voice,
  Key,
  Action,
  #[serde(rename = "settings")]
  Storage(StockStorageRecv),
  Device,
  Log,
}

// --- sends ---
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum SendMessage {
  Stock(StockSend),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged, rename_all = "snake_case")]
pub enum StockSend {
  Bluetooth(StockBluetoothSend),
  Storage(StockStorageSend),
  Setup(StockSetupSend),
}

pub struct MsgBuilder;

impl MsgBuilder {}
