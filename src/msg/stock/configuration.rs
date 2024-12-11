use serde::{Deserialize, Serialize};

use crate::msg::{PossibleSendMsg, StockSendMsg};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StockConfigurationSend {
  #[serde(rename = "remote_configuration_update")]
  Update {
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

impl From<StockConfigurationSend> for StockSendMsg {
  fn from(val: StockConfigurationSend) -> Self {
    Self::Configuration(val)
  }
}

impl From<StockConfigurationSend> for PossibleSendMsg {
  fn from(val: StockConfigurationSend) -> Self {
    Self::Stock(StockSendMsg::Configuration(val))
  }
}
