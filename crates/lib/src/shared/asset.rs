//! Retention policy for entries in the daemon's asset cache.
//!
//! Companion-supplied assets travel with a retention hint that tells the
//! daemon how long to keep the bytes around. The cache enforces this:
//! `Lru` participates in the global memory LRU; `Pinned` is held until an
//! explicit `Clear` (with a budget backstop that warn-logs if exceeded);
//! `Ttl` auto-expires; `Persistent` writes through to sqlite and survives
//! daemon restart.
//!
//! Default on the wire when omitted is `Lru` - the smallest blast radius
//! when a producer is sloppy.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

#[typeshare]
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum AssetRetention {
  #[default]
  Lru,
  Pinned,
  Ttl(TtlRetention),
  Persistent,
}

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct TtlRetention {
  pub seconds: u32,
}
