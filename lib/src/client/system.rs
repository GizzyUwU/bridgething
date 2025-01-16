use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "action", content = "args", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub enum ClientSystemCommand {
  VersionRequest,

  Reboot,
  PowerOff,
  FactoryReset,

  PhoneCallAccept { call_id: String },
  PhoneCallEnd { call_id: String },

  __LegacyStockReturnToSpotify,
  __LegacyStockRemoteConfigurationRequest,
}
