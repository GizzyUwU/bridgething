use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

/// Source of a webapp bundle. Built-in apps live in the read-only image
/// (rootfs) and cannot be uninstalled. Installed apps live on the data
/// partition and shadow built-ins of the same name.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum WebappSource {
  Builtin,
  Installed,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct WebappInfo {
  pub name: String,
  pub source: WebappSource,
  pub version: Option<String>,
  pub description: Option<String>,
}
