use serde::{Deserialize, Serialize};

use super::{SendMessage, StockSend};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum StockBluetoothRecv {
  List,
  Select { mac: String },
  Scan,
  Pair { mac: String },
  Forget { mac: String },
  Discoverable { active: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StockBluetoothSend {
  #[serde(rename = "bluetooth_connection_status")]
  ConnectionStatus { connected: bool },
  #[serde(rename = "bluetooth_local_device")]
  LocalDevice { mac: String, name: String },
  #[serde(rename = "bluetooth_current_device")]
  CurrentDevice { mac: String, name: String },
  #[serde(rename = "bluetooth_pairing_finished")]
  PairingFinished { success: bool },
  #[serde(rename = "bluetooth_pin")]
  Pin { pin: String },
  #[serde(rename = "bluetooth_device_list")]
  DeviceList { payload: Vec<StockDevice> },
}

impl From<StockBluetoothSend> for SendMessage {
  fn from(val: StockBluetoothSend) -> Self {
    SendMessage::Stock(StockSend::Bluetooth(val))
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockDevice {
  pub address: String, // mac address
  pub default: bool,
  pub device_info: StockDeviceInfo,
}

impl From<Vec<StockDevice>> for SendMessage {
  fn from(payload: Vec<StockDevice>) -> Self {
    SendMessage::Stock(StockSend::Bluetooth(StockBluetoothSend::DeviceList { payload }))
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockDeviceInfo {
  name: String,
  #[serde(rename = "type")]
  device_type: String,
}

#[cfg(test)]
mod test {
  use super::StockBluetoothRecv;
  use crate::ws::StockRecv;

  #[test]
  fn ser_stock_recv() {
    let ser = serde_json::to_string(&StockRecv::Bluetooth(StockBluetoothRecv::Discoverable { active: true }))
      .expect("failed to serialize json");
    println!("{:?}", &ser);

    assert_eq!(ser, r#"{"type":"bluetooth","action":"discoverable","active":true}"#);
  }

  #[test]
  fn de_stock_recv() {
    let json = r#"{ "type": "bluetooth", "action": "discoverable", "active": true }"#;
    let de: StockRecv = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      StockRecv::Bluetooth(StockBluetoothRecv::Discoverable { active: true })
    );
  }
}
