//! System observability surface — diagnostics snapshot + log tailing.
//! `Diagnostics` is the one-shot health snapshot; `LogEntry` is the
//! item flowing on subscribed log streams.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

/// Daemon health snapshot. `load_avg` is the unix 1/5/15-minute load.
/// `soc_temp_c` may be `None` on builds where the SoC thermal probe is
/// not exposed by the kernel.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct Diagnostics {
  pub disk_used_bytes: u32,
  pub disk_free_bytes: u32,
  pub mem_used_bytes: u32,
  pub mem_avail_bytes: u32,
  pub uptime_s: u32,
  pub soc_temp_c: Option<f32>,
  pub load_avg: [f32; 3],
  pub daemon_version: String,
  pub kernel_version: String,
  pub boot_id: String,
}

#[typeshare]
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum LogLevel {
  Trace,
  Debug,
  #[default]
  Info,
  Warn,
  Error,
}

/// What stream of log records a subscription pulls from. `Daemon` is the
/// bridgething tracing subscriber; `System` is the `journald` view; `All`
/// merges both in arrival order.
#[typeshare]
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum LogSource {
  #[default]
  Daemon,
  System,
  All,
}

/// One log record. `ts_unix_s` is unix-epoch seconds. `target` is the
/// tracing target / unit name; `message` is the rendered single-line
/// body. Pre-filtered at subscription time so wire-bloating trace
/// events don't reach webapps.
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct LogEntry {
  pub ts_unix_s: u32,
  pub level: LogLevel,
  pub target: String,
  pub message: String,
}
