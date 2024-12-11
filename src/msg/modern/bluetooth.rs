use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::msg::SendMsgData;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", content = "args", rename_all = "camelCase")]
pub enum BluetoothRecv {
  List,
  Connect { mac: String },
  Scan,
  EnableDiscoverable,
  DisableDiscoverable,
  Pair { mac: String },
  Forget { mac: String },
  EnablePAN { mac: String },
  DisablePAN { mac: String },
  SetAlias { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", content = "data", rename_all = "camelCase")]
pub enum BluetoothSend {
  Status {
    connected: bool,
  },
  ConnectedDevice {
    name: String,
    mac: String,
  },
  Interface {
    mac: String,
    name: String,
    interface: String,
  },
  ParingResult {
    success: bool,
  },
  Pin {
    mac: String,
    name: String,
    pin: String,
  },
  PairedDevices(HashMap<String, Device>),
}

impl From<BluetoothSend> for SendMsgData {
  fn from(val: BluetoothSend) -> Self {
    SendMsgData::Bluetooth(val)
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Device {
  pub name: String,
  #[serde(rename = "type")]
  pub device_type: DeviceType,
  pub mac: String,
  pub default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DeviceType {
  Android,
  #[serde(rename = "iOS")]
  Ios,
  Windows,
  MacOS,
  Linux,
  Unknown,
}
