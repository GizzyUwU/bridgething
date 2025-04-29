use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{PhoneCallDirection, PhoneCallStatus};

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(
  tag = "action",
  content = "data",
  rename_all = "camelCase",
  rename_all_fields = "camelCase"
)]
#[ts(export, export_to = "server.ts")]
pub enum ServerSystemEvent {
  Version {
    serial: String,
    os_version: String,
    app_version: String,
    fw_version: String,
    model_name: String,
    fcc_id: String,
    ic_id: String,
    country: String,
    discord: String,
    credits: String,
  },
  GatewayStatus {
    connected: bool,
    version: String,
    app: String,
  },

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
