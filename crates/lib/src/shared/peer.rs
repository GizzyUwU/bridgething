//! Live runtime view of a paired or transient counterpart of the
//! daemon. A `Peer` is the abstraction over "the thing on the other
//! side" - a phone today, a desktop later - identified by its
//! Bluetooth address and tracked through three orthogonal dimensions:
//! BlueZ pairing, the iAP2 control session (iOS-only), and the
//! bridgething companion app's gateway protocol.
//!
//! Persistence is separate. The daemon's `last_device` and known-
//! device set survive restarts; this struct does not. On boot every
//! peer starts unobserved; each transport's manager fills in its own
//! axis as connections come up.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Device, GatewayInfo};

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct Peer {
  pub device: Device,
  pub paired: bool,
  pub iap2: PeerIap2Status,
  pub companion: PeerCompanionStatus,
  pub display_name: Option<String>,
  pub language: Option<String>,
  pub uuid: Option<String>,
}

impl Peer {
  pub fn new(device: Device) -> Self {
    Self {
      device,
      paired: false,
      iap2: PeerIap2Status::None,
      companion: PeerCompanionStatus::None,
      display_name: None,
      language: None,
      uuid: None,
    }
  }

  /// True when this peer has any data channel actively producing
  /// state for the daemon: iAP2 reaching Identified (NowPlaying
  /// flowing) or the bridgething companion gateway being handshaked.
  pub fn has_useful_link(&self) -> bool {
    matches!(self.iap2, PeerIap2Status::Identified) || matches!(self.companion, PeerCompanionStatus::Connected { .. })
  }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum PeerIap2Status {
  #[default]
  None,
  LinkUp,
  Authenticated,
  Identified,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum PeerCompanionStatus {
  #[default]
  None,
  Pending,
  Connected(GatewayInfo),
}
