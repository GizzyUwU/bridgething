use serde::{Deserialize, Serialize};

use crate::msg::{PossibleSendMsg, StockSendMsg, SystemRecv};

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

impl From<StockDeviceRecv> for SystemRecv {
  fn from(data: StockDeviceRecv) -> Self {
    match data {
      StockDeviceRecv::Reboot => SystemRecv::Reboot,
      StockDeviceRecv::PowerOff => SystemRecv::PowerOff,
      StockDeviceRecv::FactoryReset => SystemRecv::FactoryReset,
      StockDeviceRecv::ReturnToSpotify => SystemRecv::__LegacyStockReturnToSpotify,
      StockDeviceRecv::PhoneCallAnswer { attributes } => SystemRecv::PhoneCallAccept {
        call_id: attributes.call_id,
      },
      StockDeviceRecv::PhoneCallEnd { attributes } => SystemRecv::PhoneCallEnd {
        call_id: attributes.call_id,
      },
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

impl From<StockHardwareSend> for PossibleSendMsg {
  fn from(val: StockHardwareSend) -> Self {
    PossibleSendMsg::Stock(StockSendMsg::Hardware(val))
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PhoneCallStatus {
  Disconnected,
  Sending,
  Ringing,
  Connecting,
  Active,
  Held,
  Disconnecting,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PhoneCallDirection {
  Incoming,
  Outgoing,
}

impl From<StockPhoneCallSend> for PossibleSendMsg {
  fn from(val: StockPhoneCallSend) -> Self {
    PossibleSendMsg::Stock(StockSendMsg::PhoneCall(val))
  }
}
