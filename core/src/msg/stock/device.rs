use libbridgething::{client::ClientSystemCommand, PhoneCallDirection, PhoneCallStatus};
use serde::{Deserialize, Serialize};

use crate::msg::{PossibleSendMsg, StockSendMsg};

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

impl From<StockDeviceRecv> for ClientSystemCommand {
  fn from(data: StockDeviceRecv) -> Self {
    match data {
      StockDeviceRecv::Reboot => ClientSystemCommand::Reboot,
      StockDeviceRecv::PowerOff => ClientSystemCommand::PowerOff,
      StockDeviceRecv::FactoryReset => ClientSystemCommand::FactoryReset,
      StockDeviceRecv::ReturnToSpotify => ClientSystemCommand::__LegacyStockReturnToSpotify,
      StockDeviceRecv::PhoneCallAnswer { attributes } => ClientSystemCommand::PhoneCallAccept {
        call_id: attributes.call_id,
      },
      StockDeviceRecv::PhoneCallEnd { attributes } => ClientSystemCommand::PhoneCallEnd {
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
