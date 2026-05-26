//! Wire-level priority lane for outbound frames. Senders annotate
//! their messages and gateway writers drain Normal preferentially,
//! filling remaining wire space with Bulk on every batch. The hint is
//! one byte at codec header offset 5; a zero byte decodes as Normal.
//!
//! Producers of large payloads (file transfer, OTA) chunk at the
//! application layer into many small typed messages tagged Bulk -
//! normal-priority traffic interleaves between frames without any
//! reassembly state on the receiver side.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

#[typeshare]
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum Priority {
  #[default]
  Normal,
  Bulk,
}

impl Priority {
  pub const fn as_byte(self) -> u8 {
    match self {
      Self::Normal => 0x00,
      Self::Bulk => 0x01,
    }
  }

  pub const fn from_byte(byte: u8) -> Self {
    match byte {
      0x01 => Self::Bulk,
      _ => Self::Normal,
    }
  }
}
