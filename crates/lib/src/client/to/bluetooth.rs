use std::collections::HashMap;

use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Device;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Whether a companion phone is currently connected over bluetooth.
pub struct BluetoothStatus {
  pub connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// The bluetooth device currently connected to the daemon.
pub struct ConnectedDevice {
  pub name: String,
  pub mac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Describes the device's own bluetooth adapter. `interface` is the
/// host-side interface name (e.g. `hci0`).
pub struct BluetoothInterface {
  pub mac: String,
  pub name: String,
  pub interface: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Outcome of an in-flight pairing attempt initiated by a peer device.
pub struct BluetoothPairingResult {
  pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// A pairing PIN a peer device is displaying, for a webapp to show as
/// on-screen confirmation.
pub struct BluetoothPin {
  pub mac: String,
  pub name: String,
  pub pin: String,
}

/// Map of MAC string to `Device`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(transparent)]
#[ts(export, export_to = "client.ts")]
pub struct PairedDevicesMap(pub HashMap<String, Device>);

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::BridgeToClientMsgData)]
/// Daemon -> webapp bluetooth surface: connection status/events,
/// in-flight pairing feedback, and the reply to `bluetooth.list`.
pub enum BridgeToClientBluetoothMsg {
  #[bridge_event]
  Status(BluetoothStatus),
  #[bridge_event]
  ConnectedDevice(ConnectedDevice),
  #[bridge_event]
  Interface(BluetoothInterface),
  #[bridge_event]
  PairingResult(BluetoothPairingResult),
  #[bridge_event]
  Pin(BluetoothPin),
  #[bridge_response]
  PairedDevices(PairedDevicesMap),
}
