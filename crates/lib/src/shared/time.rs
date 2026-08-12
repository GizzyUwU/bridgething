//! Time surface - the device has no RTC backed by a battery, so wall
//! clock authority lives with the connected companion or with iOS over
//! iAP2's DeviceTimeUpdate. Initial snapshot arrives at announce; updates
//! broadcast on tz / locale / clock skew.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Wall clock + locale snapshot. `wall_clock_unix_s` is the gateway's
/// (or iAP2 device's) claimed "now" in unix-epoch seconds - webapps
/// reading time should use the device clock if any but use this as the
/// trust anchor on first arrival.
///
/// Two zone-identification paths coexist: companion gateways send
/// `tz_iana` (an IANA zone identifier like `America/Denver`) while iAP2
/// `DeviceTimeUpdate` only exposes numeric `utc_offset_minutes` plus a
/// separate `dst_offset_minutes`. Webapps prefer `tz_iana` when present
/// and fall back to the offset pair.
#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct TimeInfo {
  pub tz_iana: Option<String>,
  pub locale: Option<String>,
  pub wall_clock_unix_s: Option<u32>,
  pub utc_offset_minutes: Option<i16>,
  pub dst_offset_minutes: Option<i8>,
}
