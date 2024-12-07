use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

pub mod stock;
use stock::*;

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
  StockInterApp { msg_id: usize, method: StockInterAppRecv },
  ConnectionClosed(tokio_websockets::CloseCode, String),
  Error(tokio_websockets::Error),
}

impl From<RecvMessage> for RecvMessageWithMeta {
  fn from(recv: RecvMessage) -> Self {
    match recv {
      RecvMessage::Stock(msg) => RecvMessageWithMeta::Stock(msg),
      RecvMessage::StockInterApp { msg_id, method } => RecvMessageWithMeta::StockInterApp { msg_id, method },
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum RecvMessage {
  Stock(StockRecv),
  #[serde(rename_all = "snake_case")]
  StockInterApp {
    msg_id: usize,
    method: StockInterAppRecv,
  },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StockRecv {
  Bluetooth(StockBluetoothRecv),
  Voice(StockVoiceRecv),
  Key,
  Action(StockActionRecv),
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
  Connection(StockConnectionSend),
  Hardware(StockHardwareSend),
  PhoneCall(StockPhoneCallSend),
  Permissions(StockPermissionsSend),
  Configuration(StockConfigurationSend),
  Version(StockVersionSend),
}
