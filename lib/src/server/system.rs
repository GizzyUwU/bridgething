use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{BridgeThingMeta, PhoneCallDirection, PhoneCallStatus};

use super::ServerEventData;

#[serde_with::skip_serializing_none]
#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "server.ts")]
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

impl From<GatewayStatus> for ServerEventData {
  fn from(status: GatewayStatus) -> Self {
    ServerEventData::System(ServerSystemEvent::GatewayStatus(status))
  }
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(
  tag = "event",
  content = "data",
  rename_all = "camelCase",
  rename_all_fields = "camelCase"
)]
#[ts(export, export_to = "server.ts")]
pub enum ServerSystemEvent {
  Version(BridgeThingMeta),
  GatewayStatus(GatewayStatus),

  // TODO: do we need these?
  OtaReboot {
    delay_ms: usize,
  },
  OtaPowerOff {
    delay_ms: usize,
  },
  AmbientLightUpdate {
    brightness: usize,
  },

  PhoneCallInfo {
    remote_id: String,
    display_name: String,
    status: PhoneCallStatus,
    call_dir: PhoneCallDirection,
    call_id: String,
  },
}
