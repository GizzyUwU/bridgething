use serde::{Deserialize, Serialize};

use crate::msg::{
  PhoneCallDirection, PhoneCallStatus, SendMsgData, StockHardwareSend, StockPhoneCallSend, StockSendMsg,
  StockVersionSend,
};

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", content = "args", rename_all = "camelCase")]
pub enum SystemRecv {
  VersionRequest,

  Reboot,
  PowerOff,
  FactoryReset,

  PhoneCallAccept { call_id: String },
  PhoneCallEnd { call_id: String },

  __LegacyStockReturnToSpotify,
  __LegacyStockRemoteConfigurationRequest,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", content = "data", rename_all = "camelCase")]
pub enum SystemSend {
  Version(String),

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

impl From<SystemSend> for SendMsgData {
  fn from(val: SystemSend) -> Self {
    SendMsgData::System(val)
  }
}

impl SystemSend {
  pub fn to_stock(self) -> StockSendMsg {
    match self {
      SystemSend::Version(version) => StockSendMsg::Version(StockVersionSend::Status {
        serial: version.clone(),
        os_version: version.clone(),
        app_version: version.clone(),
        touch_fw_version: version.clone(),
        model_name: version.clone(),
        fcc_id: version.clone(),
        ic_id: version,
      }),

      SystemSend::OtaReboot { delay_ms } => StockSendMsg::Hardware(StockHardwareSend::OtaReboot {
        delay_ms: delay_ms.to_string(),
      }),
      SystemSend::OtaPowerOff { delay_ms } => StockSendMsg::Hardware(StockHardwareSend::OtaPowerOff {
        delay_ms: delay_ms.to_string(),
      }),
      SystemSend::AmbientLightUpdate { brightness } => {
        StockSendMsg::Hardware(StockHardwareSend::AmbientLightUpdate { payload: brightness })
      }

      SystemSend::PhoneCallInfo {
        remote_id,
        display_name,
        status,
        call_dir,
        call_id,
      } => StockSendMsg::PhoneCall(StockPhoneCallSend::PhoneCallInfo {
        remote_id,
        display_name,
        status,
        call_dir,
        call_id,
      }),
    }
  }
}
