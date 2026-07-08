use bridgething_macros::{BridgeDispatch, BridgeEnum, WireRequest};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Marker request: webapp asks the bridge for the paired bluetooth
/// devices map. Pairs with `BridgeToClientBluetoothMsg::PairedDevices`.
#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = ClientToBridge,
  surface = Bluetooth,
  request_variant = List,
  response = crate::client::PairedDevicesMap,
  response_variant = PairedDevices,
)]
pub struct ListBluetoothDevices;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Payload for `bluetooth.connect`: connect to an already-paired
/// device by MAC.
pub struct ConnectBluetooth {
  pub mac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Payload for `bluetooth.forget`: unpair a device and drop it from
/// the daemon's known-device set.
pub struct ForgetBluetooth {
  pub mac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Payload for `bluetooth.setAlias`: rename the device's own adapter
/// as seen by peers during discovery and pairing.
pub struct SetBluetoothAlias {
  pub name: String,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum, BridgeDispatch)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::ClientToBridgeMsgData)]
/// Webapp -> daemon bluetooth surface: adapter alias, discoverable
/// state, and paired-device management. The device is the peripheral
/// during pairing (it never scans for new devices); `connect` only
/// re-establishes a link to an already-paired device.
pub enum ClientToBridgeBluetoothMsg {
  #[bridge_request]
  List,
  #[bridge_command]
  Connect(ConnectBluetooth),
  #[bridge_command]
  EnableDiscoverable,
  #[bridge_command]
  DisableDiscoverable,
  #[bridge_command]
  Forget(ForgetBluetooth),
  #[bridge_command]
  SetAlias(SetBluetoothAlias),
}
