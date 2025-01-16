#![allow(clippy::large_enum_variant)]
use libbridgething::{
  client::{
    ClientBluetoothCommand, ClientInteractionCommand, ClientStorageCommand, ClientSystemCommand, ClientVoiceCommand,
  },
  ClientCommand, ClientCommandType, ServerEvent, ServerEventData,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use uuid::Uuid;

pub(crate) mod stock;
use stock::*;

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
  Bluetooth(ClientBluetoothCommand),
  Storage(ClientStorageCommand),
  System(ClientSystemCommand),
  Voice(ClientVoiceCommand),
  Interaction {
    msg: ClientInteractionCommand,
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

impl From<ClientCommand> for RecvMsgData {
  fn from(msg: ClientCommand) -> Self {
    match msg.data {
      ClientCommandType::Bluetooth(msg) => Self::Bluetooth(msg),
      ClientCommandType::Storage(msg) => Self::Storage(msg),
      ClientCommandType::System(msg) => Self::System(msg),
      ClientCommandType::Voice(msg) => Self::Voice(msg),
      ClientCommandType::Interaction { msg, stock_msg_id } => Self::Interaction { msg, stock_msg_id },
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum PossibleRecvMsg {
  Modern(ClientCommand),
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
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(untagged)]
pub enum PossibleSendMsg {
  Modern(ServerEvent),
  Stock(StockSendMsg),
}

impl PossibleSendMsg {
  pub fn from_send_msg(msg: ServerEvent, mode: &ClientMode) -> Self {
    match mode {
      ClientMode::Modern => Self::Modern(msg),
      ClientMode::Stock => Self::Stock(msg.into()),
    }
  }
}

impl From<StockSendMsg> for PossibleSendMsg {
  fn from(msg: StockSendMsg) -> Self {
    Self::Stock(msg)
  }
}
