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
pub struct ConnectBluetooth {
  pub mac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PairBluetooth {
  pub mac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct ForgetBluetooth {
  pub mac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct EnablePan {
  pub mac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct DisablePan {
  pub mac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct SetBluetoothAlias {
  pub name: String,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum, BridgeDispatch)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::ClientToBridgeMsgData)]
pub enum ClientToBridgeBluetoothMsg {
  #[bridge_request]
  List,
  #[bridge_command]
  Connect(ConnectBluetooth),
  #[bridge_command]
  Scan,
  #[bridge_command]
  EnableDiscoverable,
  #[bridge_command]
  DisableDiscoverable,
  #[bridge_command]
  Pair(PairBluetooth),
  #[bridge_command]
  Forget(ForgetBluetooth),
  #[bridge_command]
  EnablePan(EnablePan),
  #[bridge_command]
  DisablePan(DisablePan),
  #[bridge_command]
  SetAlias(SetBluetoothAlias),
}
