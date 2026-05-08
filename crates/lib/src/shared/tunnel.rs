//! Tunnel surface: raw TCP byte tunnels between the on-device SOCKS5
//! proxy and the connected companion. Webapps don't see this surface
//! directly; they get TCP-via-SOCKS5 because chromium is launched with
//! `--proxy-server=socks5://127.0.0.1:1080`. The daemon enforces a
//! per-active-webapp `net.proxy` manifest permission at SOCKS handshake
//! time before opening the gateway-side `TunnelOpen` request.

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct TunnelData {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub tunnel_id: Uuid,
  #[ts(type = "Uint8Array")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub bytes: Bytes,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct TunnelClosed {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub tunnel_id: Uuid,
  pub reason: Option<String>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum TunnelError {
  /// Companion couldn't reach the host (DNS / TCP RST / network unreachable).
  ConnectFailed { reason: String },
  /// Companion refused to open a tunnel (e.g. user-denied policy).
  PermissionDenied,
  /// Tunnel surface unavailable on this companion.
  Unavailable,
}
