use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
  Device,
  client::ClientCommandType,
  impl_client_request,
  server::{ServerBluetoothEvent, ServerEventData},
};

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(
  tag = "action",
  content = "args",
  rename_all = "camelCase",
  rename_all_fields = "camelCase"
)]
#[ts(export, export_to = "client.ts")]
pub enum ClientBluetoothCommand {
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

/// Marker request: webapp asks the bridge for the paired bluetooth devices.
/// Pairs with `ServerBluetoothEvent::PairedDevices`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ListBluetoothDevices;

impl_client_request! {
  request: ListBluetoothDevices,
  response: HashMap<String, Device>,
  encode_request:
    _r => ClientCommandType::Bluetooth(ClientBluetoothCommand::List),
  extract_response:
    ServerEventData::Bluetooth(ServerBluetoothEvent::PairedDevices(v)) => v,
  encode_response:
    v => ServerEventData::Bluetooth(ServerBluetoothEvent::PairedDevices(v)),
}
