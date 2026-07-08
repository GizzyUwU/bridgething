use bridgething_macros::{BridgeEnum, WireRequest};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Webapp asks for the current `TimeSnapshot` (wall clock + locale +
/// timezone). Most webapps don't need this - the daemon also pushes
/// `Changed` events on the same shape whenever the source updates.
#[derive(Debug, Clone, Copy, Default, WireRequest)]
#[wire_request(
  direction = ClientToBridge,
  surface = Time,
  request_variant = Get,
  response = crate::client::TimeSnapshot,
  response_variant = Snapshot,
)]
pub struct TimeGet;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::ClientToBridgeMsgData)]
/// Webapp -> daemon wall-clock surface. The device has no
/// battery-backed RTC, so time authority lives with the connected
/// companion (or with iOS over iAP2's DeviceTimeUpdate).
pub enum ClientToBridgeTimeMsg {
  #[bridge_request]
  Get,
}
