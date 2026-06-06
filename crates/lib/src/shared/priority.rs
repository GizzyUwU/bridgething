//! Wire-level priority lane for outbound frames. Senders annotate
//! their messages and gateway writers drain lanes strictly in order
//! (Normal, then Bulk, then Background), filling remaining wire space
//! opportunistically on every batch. The hint is one byte at codec
//! header offset 5; an unknown byte decodes as Normal.
//!
//! Bulk carries user-blocking large payloads (requested art, browse
//! blobs); Background carries transfers nothing user-visible waits on
//! (OTA, prefetch), so a requested asset preempts an in-flight update
//! at fragment boundaries. Producers of large payloads fragment into
//! many small typed messages on their lane - higher-priority traffic
//! interleaves between fragments.

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
  Background,
}

impl Priority {
  pub const fn as_byte(self) -> u8 {
    match self {
      Self::Normal => 0x00,
      Self::Bulk => 0x01,
      Self::Background => 0x02,
    }
  }

  pub const fn from_byte(byte: u8) -> Self {
    match byte {
      0x01 => Self::Bulk,
      0x02 => Self::Background,
      _ => Self::Normal,
    }
  }
}
