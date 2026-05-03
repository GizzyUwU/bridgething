use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{BridgeThingMeta, PhoneCallDirection, PhoneCallStatus};

#[serde_with::skip_serializing_none]
#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct GatewayStatus {
  // additional meta
  pub address: String,
  pub connected: bool,

  // received meta (GatewayMeta)
  pub adapter_version: String,
  pub lib_version: String,
  pub libbridgething_version: String,
  pub app_name: String,
  pub app_version: String,
  pub os_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct OtaReboot {
  pub delay_ms: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct OtaPowerOff {
  pub delay_ms: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct AmbientLightUpdate {
  pub brightness: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PhoneCallInfo {
  pub remote_id: String,
  pub display_name: String,
  pub status: PhoneCallStatus,
  pub call_dir: PhoneCallDirection,
  pub call_id: String,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::BridgeToClientMsgData)]
pub enum BridgeToClientSystemMsg {
  #[bridge_response]
  Version(Box<BridgeThingMeta>),
  #[bridge_response]
  GatewayStatus(GatewayStatus),
  #[bridge_event]
  OtaReboot(OtaReboot),
  #[bridge_event]
  OtaPowerOff(OtaPowerOff),
  #[bridge_event]
  AmbientLightUpdate(AmbientLightUpdate),
  #[bridge_event]
  PhoneCallInfo(PhoneCallInfo),
}
