use serde::{Deserialize, Serialize};

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
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StockRecv {
  Bluetooth(StockBluetoothRecv),
  Voice(StockVoiceRecv),
  Key,
  Action(StockActionRecv),
  #[serde(rename = "settings")]
  Storage(StockStorageRecv),
  Device,
  Log,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged, rename_all = "snake_case")]
pub enum StockSend {
  Bluetooth(StockBluetoothSend),
  Storage(StockStorageSend),
  Setup(StockSetupSend),
  Connection(StockConnectionSend),
  Hardware(StockHardwareSend),
  PhoneCall(StockPhoneCallSend),
  Permissions(StockPermissionsSend),
  Configuration(StockConfigurationSend),
  Version(StockVersionSend),
}
