use libbridgething::server::ServerSystemEvent;
use serde::{Deserialize, Serialize};

use super::{RecvMsgData, ServerEvent, ServerEventData};

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
      StockRecvMsg::Key => RecvMsgData::Hole,
      StockRecvMsg::Action(data) => data.into(),
      StockRecvMsg::Storage(data) => RecvMsgData::Store(data.into()),
      StockRecvMsg::Device(data) => data.into(),
      StockRecvMsg::Log => RecvMsgData::Hole,
    }
  }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
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

impl From<ServerEvent> for StockSendMsg {
  fn from(msg: ServerEvent) -> Self {
    match msg.data {
      ServerEventData::Bluetooth(data) => StockSendMsg::Bluetooth(data.into()),
      ServerEventData::Storage(data) => StockSendMsg::Storage(data.into()),
      ServerEventData::System(data) => data.into(),
      ServerEventData::Player(data) => StockSendMsg::InterApp(StockInterAppSend::new(msg.stock_msg_id, data.into())),
      ServerEventData::Interaction(data) => {
        StockSendMsg::InterApp(StockInterAppSend::from_interaction_send(data, msg.stock_msg_id))
      }
      ServerEventData::Ack => StockSendMsg::InterApp(StockInterAppSend::make_ack(msg.stock_msg_id)),
    }
  }
}

impl From<ServerSystemEvent> for StockSendMsg {
  fn from(value: ServerSystemEvent) -> Self {
    match value {
      ServerSystemEvent::Version {
        serial,
        os_version,
        app_version,
        fw_version,
        model_name,
        fcc_id,
        ic_id,
        country,
        discord,
        credits,
      } => StockSendMsg::Version(StockVersionSend::Status {
        serial,
        os_version,
        app_version,
        fw_version,
        model_name,
        fcc_id,
        ic_id,
        country,
        discord,
        credits,
      }),

      ServerSystemEvent::OtaReboot { delay_ms } => StockSendMsg::Hardware(StockHardwareSend::OtaReboot {
        delay_ms: delay_ms.to_string(),
      }),
      ServerSystemEvent::OtaPowerOff { delay_ms } => StockSendMsg::Hardware(StockHardwareSend::OtaPowerOff {
        delay_ms: delay_ms.to_string(),
      }),
      ServerSystemEvent::AmbientLightUpdate { brightness } => {
        StockSendMsg::Hardware(StockHardwareSend::AmbientLightUpdate { payload: brightness })
      }

      ServerSystemEvent::PhoneCallInfo {
        remote_id,
        display_name,
        status,
        call_dir,
        call_id,
      } => StockSendMsg::PhoneCall(StockPhoneCallSend::PhoneCallInfo {
        remote_id,
        display_name,
        status,
        call_dir,
        call_id,
      }),
    }
  }
}
