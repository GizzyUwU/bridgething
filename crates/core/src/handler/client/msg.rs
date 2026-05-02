#![allow(clippy::large_enum_variant)]
use std::net::SocketAddr;

use libbridgething::{
  ForwardMessage,
  client::{
    BridgeToClientMsg, ClientLegacyStockCommand, ClientToBridgeAssetMsgRequest, ClientToBridgeBluetoothMsg,
    ClientToBridgeInteractionMsgCommand, ClientToBridgeMsg, ClientToBridgeMsgData, ClientToBridgeStoreMsgRequest,
    ClientToBridgeSystemMsg, ClientToBridgeVoiceMsgCommand,
  },
};
use serde::{Deserialize, Serialize};
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
  Asset(ClientToBridgeAssetMsgRequest),
  Bluetooth(ClientToBridgeBluetoothMsg),
  Store(ClientToBridgeStoreMsgRequest),
  System(ClientToBridgeSystemMsg),
  Voice(ClientToBridgeVoiceMsgCommand),
  Interaction(ClientToBridgeInteractionMsgCommand),
  Forward(ForwardMessage),

  // stock compatibility
  LegacyStock(ClientLegacyStockCommand),

  // typed-request response: the connection layer extracts these from
  // any modern inbound message with `MsgMeta::Response { request_id }`
  // and the listener routes them to `ClientManager::complete_pending`
  // before normal handler dispatch.
  Response {
    request_id: Uuid,
    data: ClientToBridgeMsgData,
  },

  // ignored and unsupported
  Hole,
  Unsupported(PossibleRecvMsg),

  // metadata
  ChangeMode(ClientMode),

  // errors
  ConnectionClosed(u16, String),
  Error(axum::Error),
}

impl From<ClientToBridgeMsg> for RecvMsgData {
  fn from(msg: ClientToBridgeMsg) -> Self {
    match msg.data {
      ClientToBridgeMsgData::Asset(inner) => match inner.into_request() {
        Some(req) => Self::Asset(req),
        None => Self::Hole,
      },
      ClientToBridgeMsgData::Bluetooth(inner) => Self::Bluetooth(inner),
      ClientToBridgeMsgData::Store(inner) => match inner.into_request() {
        Some(req) => Self::Store(req),
        None => Self::Hole,
      },
      ClientToBridgeMsgData::System(inner) => Self::System(inner),
      ClientToBridgeMsgData::Voice(inner) => match inner.into_command() {
        Some(cmd) => Self::Voice(cmd),
        None => Self::Hole,
      },
      ClientToBridgeMsgData::Interaction(inner) => match inner.into_command() {
        Some(cmd) => Self::Interaction(cmd),
        None => Self::Hole,
      },
      ClientToBridgeMsgData::Forward(data) => Self::Forward(data),
      ClientToBridgeMsgData::LegacyStock(msg) => Self::LegacyStock(msg),
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum PossibleRecvMsg {
  Modern(ClientToBridgeMsg),
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
  Modern(BridgeToClientMsg),
  #[from]
  Stock(StockSendMsg),
}

impl PossibleSendMsg {
  /// Wrap a modern `BridgeToClientMsg` for outbound transmission.
  /// `stock_msg_id` is the inter-app correlation id for stock connections
  /// (ignored for modern).
  pub fn from_send_msg(msg: BridgeToClientMsg, mode: &ClientMode, stock_msg_id: Option<usize>) -> Self {
    match mode {
      ClientMode::Modern => Self::Modern(msg),
      ClientMode::Stock => Self::Stock(crate::stock::server_event_to_stock(msg, stock_msg_id)),
    }
  }
}
