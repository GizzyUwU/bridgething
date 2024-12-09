use serde::{Deserialize, Serialize};

use crate::msg::{
  PhoneCallDirection, PhoneCallStatus, SendMsgData, StockConfigurationSend, StockConnectionSend, StockHardwareSend,
  StockPermissionsSend, StockPhoneCallSend, StockSendMsg, StockSetupSend, StockVersionSend,
};

use super::DeviceType;

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

  __LegacyStockVersion {
    serial: String,
    os_version: String,
    app_version: String,
    touch_fw_version: String,
    model_name: String,
    fcc_id: String,
    ic_id: String,
  },
  __LegacyStockSetupStatus(String),
  __LegacyStockPermissionSend {
    can_use_superbird: bool,
    can_play_on_demand: bool,
  },
  __LegacyStockRemoteStatus {
    payload: bool,
    mac: String,
    phone_type: DeviceType,
  },
  __LegacyStockTransportStatus {
    payload: bool,
  },
  __LegacyStockRemoteConfigurationUpdate {
    developer_menu_enabled: bool,
    batch_ubi_logs: bool,
    log_requests: bool,
    log_signal_strength: bool,
    podcast_trailer_enabled: bool,
    use_superbird_namespace: bool,
    use_volume_superbird_namespace: bool,
    handle_incoming_phone_calls: bool,
    night_mode_strength: usize,
    night_mode_slope: usize,
    graphql_endpoint_enabled: bool,
    enable_push_to_talk_shelf: bool,
    non_spotify_playback_ios: bool,
    graphql_for_shelf_enabled: bool,
    sunset_info_screen: bool,
    sunset_kill_switch: bool,
    sunset_info_screen_nag: bool,
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

      Self::__LegacyStockVersion {
        serial,
        os_version,
        app_version,
        touch_fw_version,
        model_name,
        fcc_id,
        ic_id,
      } => StockSendMsg::Version(StockVersionSend::Status {
        serial,
        os_version,
        app_version,
        touch_fw_version,
        model_name,
        fcc_id,
        ic_id,
      }),
      SystemSend::__LegacyStockSetupStatus(payload) => StockSendMsg::Setup(StockSetupSend::Status { payload }),
      SystemSend::__LegacyStockPermissionSend {
        can_use_superbird,
        can_play_on_demand,
      } => StockSendMsg::Permissions(StockPermissionsSend::DevicePermissions {
        can_use_superbird,
        can_play_on_demand,
      }),
      SystemSend::__LegacyStockTransportStatus { payload } => {
        StockSendMsg::Connection(StockConnectionSend::TransportStatus { payload })
      }
      SystemSend::__LegacyStockRemoteStatus {
        payload,
        mac,
        phone_type,
      } => StockSendMsg::Connection(StockConnectionSend::RemoteStatus {
        payload,
        mac,
        phone_type: phone_type.into(),
      }),
      SystemSend::__LegacyStockRemoteConfigurationUpdate {
        developer_menu_enabled,
        batch_ubi_logs,
        log_requests,
        log_signal_strength,
        podcast_trailer_enabled,
        use_superbird_namespace,
        use_volume_superbird_namespace,
        handle_incoming_phone_calls,
        night_mode_strength,
        night_mode_slope,
        graphql_endpoint_enabled,
        enable_push_to_talk_shelf,
        non_spotify_playback_ios,
        graphql_for_shelf_enabled,
        sunset_info_screen,
        sunset_kill_switch,
        sunset_info_screen_nag,
      } => StockSendMsg::Configuration(StockConfigurationSend::Update {
        developer_menu_enabled,
        batch_ubi_logs,
        log_requests,
        log_signal_strength,
        podcast_trailer_enabled,
        use_superbird_namespace,
        use_volume_superbird_namespace,
        handle_incoming_phone_calls,
        night_mode_strength,
        night_mode_slope,
        graphql_endpoint_enabled,
        enable_push_to_talk_shelf,
        non_spotify_playback_ios,
        graphql_for_shelf_enabled,
        sunset_info_screen,
        sunset_kill_switch,
        sunset_info_screen_nag,
      }),
    }
  }
}
