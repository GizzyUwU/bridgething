use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Device;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(
  tag = "event",
  content = "data",
  rename_all = "camelCase",
  rename_all_fields = "camelCase"
)]
#[ts(export, export_to = "server.ts")]
pub enum ServerBluetoothEvent {
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
