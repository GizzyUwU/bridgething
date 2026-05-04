use base64::Engine as _;
use libbridgething::{
  BridgeThingMeta,
  client::{
    AmbientLightUpdate, BridgeToClientAssetMsg, BridgeToClientHardwareMsg, BridgeToClientMsg, BridgeToClientMsgData,
    BridgeToClientSystemMsg,
  },
  transitive_from,
};
use serde::{Deserialize, Serialize};

use crate::handler::client::{PossibleSendMsg, RecvMsgData};

mod action;
mod bluetooth;
mod configuration;
mod connection;
mod device;
mod interapp;
mod messages;
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
pub use messages::*;
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

#[derive(Debug, Clone, Serialize, PartialEq, derive_more::From)]
#[serde(untagged, rename_all = "camelCase")]
pub enum StockSendMsg {
  #[from]
  Bluetooth(StockBluetoothSend),
  #[from]
  Storage(StockStorageSend),
  #[from]
  Setup(StockSetupSend),
  #[from]
  Connection(StockConnectionSend),
  #[from]
  Hardware(StockHardwareSend),
  #[from]
  PhoneCall(StockPhoneCallSend),
  #[from]
  Permissions(StockPermissionsSend),
  #[from]
  Configuration(StockConfigurationSend),
  #[from]
  Version(StockVersionSend),
  #[from]
  Voice(StockVoiceSend),
  #[from]
  InterApp(StockInterAppSend),
  Unsupported,
}

pub fn server_event_to_stock(msg: BridgeToClientMsg, stock_msg_id: Option<usize>) -> StockSendMsg {
  match msg.data {
    BridgeToClientMsgData::Bluetooth(data) => StockSendMsg::Bluetooth(data.into()),
    BridgeToClientMsgData::Store(data) => StockSendMsg::Storage(data.into()),
    BridgeToClientMsgData::System(data) => data.into(),
    BridgeToClientMsgData::Player(data) => StockSendMsg::InterApp(StockInterAppSend::new(stock_msg_id, data.into())),
    BridgeToClientMsgData::Hardware(BridgeToClientHardwareMsg::AmbientLightUpdate(AmbientLightUpdate {
      brightness,
    })) => StockSendMsg::Hardware(StockHardwareSend::AmbientLightUpdate {
      payload: brightness as usize,
    }),
    BridgeToClientMsgData::Forward(_) => {
      tracing::warn!("forward message is not supported in stock app!!");
      StockSendMsg::InterApp(StockInterAppSend::make_ack(stock_msg_id))
    }
    BridgeToClientMsgData::Error(err) => {
      tracing::warn!("typed error response is not supported in stock app: {:?}", err);
      StockSendMsg::Unsupported
    }
    BridgeToClientMsgData::Asset(data) => match data {
      BridgeToClientAssetMsg::Got(got) => {
        let image_data = base64::engine::general_purpose::STANDARD.encode(&got.bytes);
        StockSendMsg::InterApp(StockInterAppSend::new(
          stock_msg_id,
          StockInterAppSendPayload::Image {
            height: 0,
            width: 0,
            image_data,
          },
        ))
      }
      BridgeToClientAssetMsg::NotFound(_) | BridgeToClientAssetMsg::Ready(_) | BridgeToClientAssetMsg::Cleared(_) => {
        StockSendMsg::InterApp(StockInterAppSend::make_ack(stock_msg_id))
      }
    },
    BridgeToClientMsgData::Ack | BridgeToClientMsgData::Done => {
      StockSendMsg::InterApp(StockInterAppSend::make_ack(stock_msg_id))
    }

    // Surfaces with no stock equivalent yet
    BridgeToClientMsgData::Audio(_)
    | BridgeToClientMsgData::Capabilities(_)
    | BridgeToClientMsgData::Config(_)
    | BridgeToClientMsgData::Geo(_)
    | BridgeToClientMsgData::Hardware(_)
    | BridgeToClientMsgData::Library(_)
    | BridgeToClientMsgData::Net(_)
    | BridgeToClientMsgData::Notifications(_)
    | BridgeToClientMsgData::Peer(_)
    | BridgeToClientMsgData::Phone(_)
    | BridgeToClientMsgData::Time(_)
    | BridgeToClientMsgData::Voice(_)
    | BridgeToClientMsgData::Webapp(_) => StockSendMsg::Unsupported,
  }
}

impl From<BridgeToClientSystemMsg> for StockSendMsg {
  fn from(value: BridgeToClientSystemMsg) -> Self {
    match value {
      BridgeToClientSystemMsg::Version(meta) => {
        let BridgeThingMeta {
          serial_number,
          os_version,
          app_version,
          model_name,
          fcc_id,
          ic_id,
          discord,
          credits,
          ..
        } = *meta;
        StockSendMsg::Version(StockVersionSend::Status {
          serial: serial_number,
          os_version,
          app_version,
          fw_version: "BridgeThing".to_string(),
          model_name,
          fcc_id,
          ic_id,
          country: "ThingLabs".to_string(),
          discord,
          credits,
        })
      }
      BridgeToClientSystemMsg::DiagnosticsReply(_)
      | BridgeToClientSystemMsg::LogsTailReply(_)
      | BridgeToClientSystemMsg::LogsSubscribeReply(_)
      | BridgeToClientSystemMsg::LogEntry(_) => StockSendMsg::Unsupported,
    }
  }
}

transitive_from! {
  StockBluetoothSend     => PossibleSendMsg: |v| PossibleSendMsg::Stock(StockSendMsg::Bluetooth(v)),
  StockStorageSend       => PossibleSendMsg: |v| PossibleSendMsg::Stock(StockSendMsg::Storage(v)),
  StockSetupSend         => PossibleSendMsg: |v| PossibleSendMsg::Stock(StockSendMsg::Setup(v)),
  StockConnectionSend    => PossibleSendMsg: |v| PossibleSendMsg::Stock(StockSendMsg::Connection(v)),
  StockHardwareSend      => PossibleSendMsg: |v| PossibleSendMsg::Stock(StockSendMsg::Hardware(v)),
  StockPhoneCallSend     => PossibleSendMsg: |v| PossibleSendMsg::Stock(StockSendMsg::PhoneCall(v)),
  StockPermissionsSend   => PossibleSendMsg: |v| PossibleSendMsg::Stock(StockSendMsg::Permissions(v)),
  StockConfigurationSend => PossibleSendMsg: |v| PossibleSendMsg::Stock(StockSendMsg::Configuration(v)),
  StockVersionSend       => PossibleSendMsg: |v| PossibleSendMsg::Stock(StockSendMsg::Version(v)),
  StockVoiceSend         => PossibleSendMsg: |v| PossibleSendMsg::Stock(StockSendMsg::Voice(v)),
  StockInterAppSend      => PossibleSendMsg: |v| PossibleSendMsg::Stock(StockSendMsg::InterApp(v)),
  StockStoragePayload    => PossibleSendMsg: |payload| PossibleSendMsg::Stock(StockSendMsg::Storage(StockStorageSend::Response { payload })),
  Vec<StockDevice>       => PossibleSendMsg: |payload| PossibleSendMsg::Stock(StockSendMsg::Bluetooth(StockBluetoothSend::DeviceList { payload })),
}
