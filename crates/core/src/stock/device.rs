use libbridgething::{
  PhoneCallDirection, PhoneCallStatus,
  client::{ClientToBridgeSystemMsg, PhoneCallAccept, PhoneCallEnd},
};
use serde::{Deserialize, Serialize};

use crate::handler::client::RecvMsgData;

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
      StockDeviceRecv::Reboot => RecvMsgData::System(ClientToBridgeSystemMsg::Reboot),
      StockDeviceRecv::PowerOff => RecvMsgData::System(ClientToBridgeSystemMsg::PowerOff),
      StockDeviceRecv::FactoryReset => RecvMsgData::System(ClientToBridgeSystemMsg::FactoryReset),
      StockDeviceRecv::PhoneCallAnswer { attributes } => {
        RecvMsgData::System(ClientToBridgeSystemMsg::PhoneCallAccept(PhoneCallAccept {
          call_id: attributes.call_id,
        }))
      }
      StockDeviceRecv::PhoneCallEnd { attributes } => {
        RecvMsgData::System(ClientToBridgeSystemMsg::PhoneCallEnd(PhoneCallEnd {
          call_id: attributes.call_id,
        }))
      }
      StockDeviceRecv::ReturnToSpotify => RecvMsgData::Unsupported(crate::handler::client::PossibleRecvMsg::Stock(
        super::StockRecvMsg::Device(data),
      )),
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
