use serde::{Deserialize, Serialize};

use super::{RecvMsgData, SendMsg, SendMsgData};

mod action;
mod bluetooth;
mod configuration;
mod connection;
mod device;
mod interapp;
mod permissions;
mod settings;
mod setup;
mod version;
mod voice;

pub use action::*;
pub use bluetooth::*;
pub use configuration::*;
pub use connection::*;
pub use device::*;
pub use interapp::*;
pub use permissions::*;
pub use settings::*;
pub use setup::*;
pub use version::*;
pub use voice::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum StockRecvMsg {
  Bluetooth(StockBluetoothRecv),
  Voice(StockVoiceRecv),
  Key,
  Action(StockActionRecv),
  #[serde(rename = "settings")]
  Storage(StockStorageRecv),
  Device(StockDeviceRecv),
  Log,
}

impl From<StockRecvMsg> for RecvMsgData {
  fn from(msg: StockRecvMsg) -> Self {
    match msg {
      StockRecvMsg::Bluetooth(data) => RecvMsgData::Bluetooth(data.into()),
      StockRecvMsg::Voice(data) => RecvMsgData::Voice(data.into()),
      StockRecvMsg::Key => RecvMsgData::Hole(None),
      StockRecvMsg::Action(data) => RecvMsgData::System(data.into()),
      StockRecvMsg::Storage(data) => RecvMsgData::Storage(data.into()),
      StockRecvMsg::Device(data) => RecvMsgData::System(data.into()),
      StockRecvMsg::Log => RecvMsgData::Hole(None),
    }
  }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(untagged, rename_all = "camelCase")]
pub enum StockSendMsg {
  Bluetooth(StockBluetoothSend),
  Storage(StockStorageSend),
  Setup(StockSetupSend),
  Connection(StockConnectionSend),
  Hardware(StockHardwareSend),
  PhoneCall(StockPhoneCallSend),
  Permissions(StockPermissionsSend),
  Configuration(StockConfigurationSend),
  Version(StockVersionSend),
  Voice(StockVoiceSend),
  InterApp(StockInterAppSend),
}

impl From<SendMsg> for StockSendMsg {
  fn from(msg: SendMsg) -> Self {
    match msg.data {
      SendMsgData::Bluetooth(data) => StockSendMsg::Bluetooth(data.into()),
      SendMsgData::Storage(data) => StockSendMsg::Storage(data.into()),
      SendMsgData::System(data) => data.to_stock(),
      SendMsgData::Interaction(data) => StockSendMsg::InterApp(data.to_stock(msg.stock_msg_id)),
      SendMsgData::Ack => StockSendMsg::InterApp(StockInterAppSend::make_ack(msg.stock_msg_id)),
    }
  }
}
