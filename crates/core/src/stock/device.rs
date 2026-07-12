use libbridgething::{
  PhoneCallDirection, PhoneCallStatus,
  client::{ClientToBridgePhoneMsg, ClientToBridgeSystemMsg, PhoneCallAction},
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
        RecvMsgData::Phone(ClientToBridgePhoneMsg::Answer(PhoneCallAction {
          call_id: attributes.call_id,
        }))
      }
      StockDeviceRecv::PhoneCallEnd { attributes } => {
        RecvMsgData::Phone(ClientToBridgePhoneMsg::End(PhoneCallAction {
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
    status: StockPhoneCallStatus,
    call_dir: StockPhoneCallDirection,
    call_id: String,
  },
}

/// The stock webapp keeps a separate call store per phone platform and picks
/// between them on the `phone_type` we report at connect. The iOS store reads
/// `phone_call_info` (above); the Android store only ever reads this message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "payload")]
pub enum StockLegacyPhoneCallSend {
  #[serde(rename = "com.spotify.superbird.phone.state")]
  PhoneState {
    state: StockLegacyPhoneCallState,
    phone_number: String,
    display_name: String,
  },
}

/// The Android store's whole vocabulary. It has no outgoing-call state at all
/// (`isRingingOutgoing` is hardcoded false), so an outgoing call stays hidden
/// until it goes `Active` and lands on `Offhook`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StockLegacyPhoneCallState {
  Idle,
  Ringing,
  Offhook,
}

impl StockLegacyPhoneCallState {
  pub fn from_call(status: &PhoneCallStatus, direction: &PhoneCallDirection) -> Self {
    match status {
      PhoneCallStatus::Active | PhoneCallStatus::Held => StockLegacyPhoneCallState::Offhook,
      PhoneCallStatus::Ringing if matches!(direction, PhoneCallDirection::Incoming) => {
        StockLegacyPhoneCallState::Ringing
      }
      _ => StockLegacyPhoneCallState::Idle,
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StockPhoneCallStatus {
  Disconnected,
  Sending,
  Ringing,
  Connecting,
  Active,
  Held,
  Disconnecting,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StockPhoneCallDirection {
  Incoming,
  Outgoing,
}

impl From<PhoneCallStatus> for StockPhoneCallStatus {
  fn from(data: PhoneCallStatus) -> Self {
    match data {
      PhoneCallStatus::Disconnected => StockPhoneCallStatus::Disconnected,
      PhoneCallStatus::Sending => StockPhoneCallStatus::Sending,
      PhoneCallStatus::Ringing => StockPhoneCallStatus::Ringing,
      PhoneCallStatus::Connecting => StockPhoneCallStatus::Connecting,
      PhoneCallStatus::Active => StockPhoneCallStatus::Active,
      PhoneCallStatus::Held => StockPhoneCallStatus::Held,
      PhoneCallStatus::Disconnecting => StockPhoneCallStatus::Disconnecting,
    }
  }
}

impl From<PhoneCallDirection> for StockPhoneCallDirection {
  fn from(data: PhoneCallDirection) -> Self {
    match data {
      PhoneCallDirection::Incoming => StockPhoneCallDirection::Incoming,
      PhoneCallDirection::Outgoing => StockPhoneCallDirection::Outgoing,
    }
  }
}

#[cfg(test)]
mod test {
  use super::*;

  #[test]
  fn phone_call_info_serializes_stock_casing() {
    let msg = StockPhoneCallSend::PhoneCallInfo {
      remote_id: "+15555550100".into(),
      display_name: "Test Caller".into(),
      status: PhoneCallStatus::Ringing.into(),
      call_dir: PhoneCallDirection::Incoming.into(),
      call_id: "call-1".into(),
    };
    let json = serde_json::to_value(&msg).expect("serialize");
    assert_eq!(json["type"], "phone_call_info");
    assert_eq!(json["status"], "Ringing");
    assert_eq!(json["call_dir"], "Incoming");
  }
}
