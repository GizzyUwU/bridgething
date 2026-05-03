//! Time surface — the device has no RTC backed by a battery, so wall
//! clock authority lives with the connected companion. Initial snapshot
//! arrives at announce; updates broadcast on tz / locale / clock skew.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

/// Wall clock + locale snapshot. `tz_iana` is the IANA zone identifier
/// (`America/Denver`, `Europe/London`); `locale` is BCP-47;
/// `wall_clock_unix_s` is the gateway's claimed "now" in unix-epoch
/// seconds — webapps reading time should use the device clock if any
/// but use this as the trust anchor on first arrival.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct TimeInfo {
  pub tz_iana: String,
  pub locale: String,
  pub wall_clock_unix_s: Option<u32>,
}
