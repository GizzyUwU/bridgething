use std::net::SocketAddr;

use libbridgething::{
  ForwardMessage,
  client::{
    BridgeToClientMsg, ClientLegacyStockCommand, ClientToBridgeAssetMsgRequest, ClientToBridgeAudioMsgCommand,
    ClientToBridgeBluetoothMsg, ClientToBridgeCapabilitiesMsgRequest, ClientToBridgeConfigMsgRequest,
    ClientToBridgeGeoMsg, ClientToBridgeHardwareMsg, ClientToBridgeLibraryMsg, ClientToBridgeMsg,
    ClientToBridgeMsgData, ClientToBridgeNetMsg, ClientToBridgeNotificationsMsg, ClientToBridgePhoneMsg,
    ClientToBridgePlayerMsg, ClientToBridgeStoreMsgRequest, ClientToBridgeSystemMsg, ClientToBridgeTimeMsgRequest,
    ClientToBridgeVoiceMsgCommand, ClientToBridgeWebappMsgRequest,
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
  Audio(ClientToBridgeAudioMsgCommand),
  Bluetooth(ClientToBridgeBluetoothMsg),
  Capabilities(ClientToBridgeCapabilitiesMsgRequest),
  Config(ClientToBridgeConfigMsgRequest),
  Geo(ClientToBridgeGeoMsg),
  Hardware(ClientToBridgeHardwareMsg),
  Library(ClientToBridgeLibraryMsg),
  Net(ClientToBridgeNetMsg),
  Notifications(ClientToBridgeNotificationsMsg),
  Phone(ClientToBridgePhoneMsg),
  Player(ClientToBridgePlayerMsg),
  Store(ClientToBridgeStoreMsgRequest),
  System(ClientToBridgeSystemMsg),
  Time(ClientToBridgeTimeMsgRequest),
  Voice(ClientToBridgeVoiceMsgCommand),
  Webapp(ClientToBridgeWebappMsgRequest),
  Forward(ForwardMessage),

  // stock compatibility
  LegacyStock(ClientLegacyStockCommand),

  // typed-request response
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
      ClientToBridgeMsgData::Audio(inner) => match inner.into_command() {
        Some(cmd) => Self::Audio(cmd),
        None => Self::Hole,
      },
      ClientToBridgeMsgData::Bluetooth(inner) => Self::Bluetooth(inner),
      ClientToBridgeMsgData::Capabilities(inner) => match inner.into_request() {
        Some(req) => Self::Capabilities(req),
        None => Self::Hole,
      },
      ClientToBridgeMsgData::Config(inner) => match inner.into_request() {
        Some(req) => Self::Config(req),
        None => Self::Hole,
      },
      ClientToBridgeMsgData::Geo(inner) => Self::Geo(inner),
      ClientToBridgeMsgData::Hardware(inner) => Self::Hardware(inner),
      ClientToBridgeMsgData::Library(inner) => Self::Library(inner),
      ClientToBridgeMsgData::Net(inner) => Self::Net(inner),
      ClientToBridgeMsgData::Notifications(inner) => Self::Notifications(inner),
      ClientToBridgeMsgData::Phone(inner) => Self::Phone(inner),
      ClientToBridgeMsgData::Player(inner) => Self::Player(inner),
      ClientToBridgeMsgData::Store(inner) => match inner.into_request() {
        Some(req) => Self::Store(req),
        None => Self::Hole,
      },
      ClientToBridgeMsgData::System(inner) => Self::System(inner),
      ClientToBridgeMsgData::Time(inner) => match inner.into_request() {
        Some(req) => Self::Time(req),
        None => Self::Hole,
      },
      ClientToBridgeMsgData::Voice(inner) => match inner.into_command() {
        Some(cmd) => Self::Voice(cmd),
        None => Self::Hole,
      },
      ClientToBridgeMsgData::Webapp(inner) => match inner.into_request() {
        Some(req) => Self::Webapp(req),
        None => Self::Hole,
      },
      ClientToBridgeMsgData::Forward(data) => Self::Forward(data),
      ClientToBridgeMsgData::LegacyStock(msg) => Self::LegacyStock(msg),
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
