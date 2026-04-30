use libbridgething::{Device, DeviceType, client::ClientBluetoothCommand, server::ServerBluetoothEvent};
use serde::{Deserialize, Serialize};

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

impl From<StockBluetoothRecv> for ClientBluetoothCommand {
  fn from(data: StockBluetoothRecv) -> Self {
    match data {
      StockBluetoothRecv::List => ClientBluetoothCommand::List,
      StockBluetoothRecv::Select { mac } => ClientBluetoothCommand::Connect { mac },
      StockBluetoothRecv::Scan => ClientBluetoothCommand::Scan,
      StockBluetoothRecv::Pair { mac } => ClientBluetoothCommand::Pair { mac },
      StockBluetoothRecv::Forget { mac } => ClientBluetoothCommand::Forget { mac },
      StockBluetoothRecv::Discoverable { active } => match active {
        true => ClientBluetoothCommand::EnableDiscoverable,
        false => ClientBluetoothCommand::DisableDiscoverable,
      },
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StockBluetoothSend {
  #[serde(rename = "bluetooth_connection_status")]
  ConnectionStatus { connected: bool },
  #[serde(rename = "bluetooth_local_device")]
  LocalDevice { mac: String, name: String },
  #[serde(rename = "bluetooth_current_device")]
  CurrentDevice { address: String, name: String },
  #[serde(rename = "bluetooth_pairing_finished")]
  PairingFinished { success: bool },
  #[serde(rename = "bluetooth_pin")]
  Pin { pin: String },
  #[serde(rename = "bluetooth_device_list")]
  DeviceList { payload: Vec<StockDevice> },
}

impl From<ServerBluetoothEvent> for StockBluetoothSend {
  fn from(data: ServerBluetoothEvent) -> Self {
    match data {
      ServerBluetoothEvent::Status { connected } => StockBluetoothSend::ConnectionStatus { connected },
      ServerBluetoothEvent::ConnectedDevice { name, mac } => StockBluetoothSend::CurrentDevice { address: mac, name },
      ServerBluetoothEvent::Interface { mac, name, .. } => StockBluetoothSend::LocalDevice { mac, name },
      ServerBluetoothEvent::ParingResult { success } => StockBluetoothSend::PairingFinished { success },
      ServerBluetoothEvent::Pin { pin, .. } => StockBluetoothSend::Pin { pin },
      ServerBluetoothEvent::PairedDevices(info) => StockBluetoothSend::DeviceList {
        payload: info.values().map(|d| d.to_owned().into()).collect(),
      },
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockDevice {
  pub address: String, // mac address
  pub default: bool,
  pub device_info: StockDeviceInfo,
}

impl From<Device> for StockDevice {
  fn from(data: Device) -> Self {
    Self {
      address: data.mac,
      default: data.default,
      device_info: StockDeviceInfo {
        name: data.name,
        device_type: data.device_type.into(),
      },
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockDeviceInfo {
  pub name: String,
  #[serde(rename = "type")]
  pub device_type: StockDeviceType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StockDeviceType {
  Android,
  #[serde(rename = "iOS")]
  Ios,
}

impl From<DeviceType> for StockDeviceType {
  fn from(data: DeviceType) -> Self {
    match data {
      DeviceType::Ios | DeviceType::MacOS => StockDeviceType::Ios,
      _ => StockDeviceType::Android,
    }
  }
}

#[cfg(test)]
mod test {
  use super::StockBluetoothRecv;
  use crate::stock::StockRecvMsg;

  #[test]
  fn ser_stock_recv() {
    let ser = serde_json::to_string(&StockRecvMsg::Bluetooth(StockBluetoothRecv::Discoverable {
      active: true,
    }))
    .expect("failed to serialize json");
    println!("{:?}", &ser);

    assert_eq!(ser, r#"{"type":"bluetooth","action":"discoverable","active":true}"#);
  }

  #[test]
  fn de_stock_recv() {
    let json = r#"{ "type": "bluetooth", "action": "discoverable", "active": true }"#;
    let de: StockRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      StockRecvMsg::Bluetooth(StockBluetoothRecv::Discoverable { active: true })
    );
  }
}
