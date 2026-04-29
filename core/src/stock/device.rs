use libbridgething::{PhoneCallDirection, PhoneCallStatus, client::ClientSystemCommand};
use serde::{Deserialize, Serialize};

use super::StockSendMsg;
use crate::handler::client::{PossibleSendMsg, RecvMsgData};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum StockDeviceRecv {
  Reboot,
  PowerOff,
  FactoryReset,
  ReturnToSpotify,

  PhoneCallAnswer { attributes: PhoneCallAttributes },
  PhoneCallEnd { attributes: PhoneCallAttributes },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhoneCallAttributes {
  call_id: String,
}

impl From<StockDeviceRecv> for RecvMsgData {
  fn from(data: StockDeviceRecv) -> Self {
    match data {
      StockDeviceRecv::Reboot => RecvMsgData::System(ClientSystemCommand::Reboot),
      StockDeviceRecv::PowerOff => RecvMsgData::System(ClientSystemCommand::PowerOff),
      StockDeviceRecv::FactoryReset => RecvMsgData::System(ClientSystemCommand::FactoryReset),
      StockDeviceRecv::PhoneCallAnswer { attributes } => RecvMsgData::System(ClientSystemCommand::PhoneCallAccept {
        call_id: attributes.call_id,
      }),
      StockDeviceRecv::PhoneCallEnd { attributes } => RecvMsgData::System(ClientSystemCommand::PhoneCallEnd {
        call_id: attributes.call_id,
      }),
      StockDeviceRecv::ReturnToSpotify => {
        RecvMsgData::Unsupported(crate::handler::client::PossibleRecvMsg::Stock(super::StockRecvMsg::Device(data)))
      }
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StockHardwareSend {
  OtaReboot { delay_ms: String },
  OtaPowerOff { delay_ms: String },
  AmbientLightUpdate { payload: usize },
}

impl From<StockHardwareSend> for StockSendMsg {
  fn from(val: StockHardwareSend) -> Self {
    Self::Hardware(val)
  }
}

impl From<StockHardwareSend> for PossibleSendMsg {
  fn from(val: StockHardwareSend) -> Self {
    Self::Stock(StockSendMsg::Hardware(val))
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StockPhoneCallSend {
  PhoneCallInfo {
    remote_id: String,
    display_name: String,
    status: PhoneCallStatus,
    call_dir: PhoneCallDirection,
    call_id: String,
  },
}

impl From<StockPhoneCallSend> for StockSendMsg {
  fn from(val: StockPhoneCallSend) -> Self {
    Self::PhoneCall(val)
  }
}

impl From<StockPhoneCallSend> for PossibleSendMsg {
  fn from(val: StockPhoneCallSend) -> Self {
    Self::Stock(StockSendMsg::PhoneCall(val))
  }
}
