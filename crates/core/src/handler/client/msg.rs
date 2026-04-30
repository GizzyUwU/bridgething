#![allow(clippy::large_enum_variant)]
use libbridgething::{
  ClientCommand, ClientCommandType, ForwardMessage, ServerEvent, ServerEventData,
  client::{
    ClientBluetoothCommand, ClientInteractionCommand, ClientKVStoreCommand, ClientLegacyStockCommand,
    ClientSystemCommand, ClientVoiceCommand,
  },
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use uuid::Uuid;

use crate::stock::{StockInterAppRecv, StockRecvMsg, StockSendMsg};

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

  pub stock_msg_id: Option<usize>,
}

#[derive(Debug)]
pub enum RecvMsgData {
  Bluetooth(ClientBluetoothCommand),
  Store(ClientKVStoreCommand),
  System(ClientSystemCommand),
  Voice(ClientVoiceCommand),
  Interaction(ClientInteractionCommand),
  Forward(ForwardMessage),

  // stock compatibility
  LegacyStock(ClientLegacyStockCommand),

  // ignored and unsupported
  Hole,
  Unsupported(PossibleRecvMsg),

  // metadata
  ChangeMode(ClientMode),

  // errors
  ConnectionClosed(u16, String),
  Error(axum::Error),
}

impl From<ClientCommand> for RecvMsgData {
  fn from(msg: ClientCommand) -> Self {
    match msg.data {
      ClientCommandType::Bluetooth(msg) => Self::Bluetooth(msg),
      ClientCommandType::Store(msg) => Self::Store(msg),
      ClientCommandType::System(msg) => Self::System(msg),
      ClientCommandType::Voice(msg) => Self::Voice(msg),
      ClientCommandType::Interaction(msg) => Self::Interaction(msg),
      ClientCommandType::Forward(data) => Self::Forward(data),

      // legacy
      ClientCommandType::LegacyStock(msg) => Self::LegacyStock(msg),
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
      PossibleRecvMsg::StockInterApp { .. } => RecvMsgData::from_stock_inter_app_possible_recv(recv),
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
#[derive(Debug, Clone, Serialize, PartialEq, derive_more::From)]
#[serde(untagged)]
pub enum PossibleSendMsg {
  #[from]
  Modern(ServerEvent),
  #[from]
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
