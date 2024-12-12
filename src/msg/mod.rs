#![allow(clippy::large_enum_variant)]
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use uuid::Uuid;

pub(crate) mod stock;
use stock::*;

mod modern;
pub use modern::*;

pub type RecvTx = tokio::sync::mpsc::Sender<RecvMsg>;
pub type RecvRx = tokio::sync::mpsc::Receiver<RecvMsg>;
pub type SendTx = tokio::sync::mpsc::Sender<PossibleSendMsg>;
pub type SendRx = tokio::sync::mpsc::Receiver<PossibleSendMsg>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientMode {
  Modern,
  Stock,
}

// --- receives ---
#[derive(Debug)]
pub struct RecvMsg {
  pub id: Uuid,
  pub from: SocketAddr,
  pub data: RecvMsgData,
}

#[derive(Debug)]
pub enum RecvMsgData {
  Bluetooth(BluetoothRecv),
  Storage(StorageRecv),
  System(SystemRecv),
  Voice(VoiceRecv),
  Interaction {
    msg: InteractionRecv,
    stock_msg_id: Option<usize>,
  },

  // stock compatibility
  Hole(Option<usize>),

  // metadata
  ChangeMode(ClientMode),

  // errors
  ConnectionClosed(tokio_websockets::CloseCode, String),
  Error(tokio_websockets::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum PossibleRecvMsg {
  Modern(ModernRecvMsg),
  Stock(StockRecvMsg),
  #[serde(rename_all = "camelCase")]
  StockInterApp {
    msg_id: usize,
    #[serde(flatten)]
    data: StockInterAppRecv,
    user_action: bool,
  },
}

impl From<PossibleRecvMsg> for RecvMsgData {
  fn from(recv: PossibleRecvMsg) -> Self {
    match recv {
      PossibleRecvMsg::Modern(msg) => msg.into(),
      PossibleRecvMsg::Stock(msg) => msg.into(),
      PossibleRecvMsg::StockInterApp { msg_id, data, .. } => (msg_id, data).into(),
    }
  }
}

impl PossibleRecvMsg {
  pub fn uuid(&self) -> uuid::Uuid {
    match self {
      PossibleRecvMsg::Modern(msg) => msg.id,
      _ => uuid::Uuid::now_v7(),
    }
  }
}

// --- sends ---
#[derive(Debug, Copy, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SendMsgMeta {
  Request,
  Response,
  Info,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SendMsg {
  pub id: Uuid,
  #[serde(flatten)]
  pub data: SendMsgData,
  pub meta: SendMsgMeta,
  pub stock_msg_id: Option<usize>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum SendMsgData {
  Bluetooth(BluetoothSend),
  Storage(StorageSend),
  System(SystemSend),
  Interaction(InteractionSend),
  Player(PlayerSend),
  Ack,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum PossibleSendMsg {
  Modern(SendMsg),
  Stock(StockSendMsg),
}

impl From<StockSendMsg> for PossibleSendMsg {
  fn from(msg: StockSendMsg) -> Self {
    Self::Stock(msg)
  }
}

impl PossibleSendMsg {
  pub fn from_send_msg(msg: SendMsg, mode: &ClientMode) -> Self {
    match mode {
      ClientMode::Modern => Self::Modern(msg),
      ClientMode::Stock => Self::Stock(msg.into()),
    }
  }
}
