use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
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

impl Default for StockConfigurationSend {
  fn default() -> Self {
    Self::Update {
      developer_menu_enabled: true,
      batch_ubi_logs: false,
      log_requests: true,
      log_signal_strength: true,
      podcast_trailer_enabled: true,
      use_superbird_namespace: true,
      use_volume_superbird_namespace: true,
      handle_incoming_phone_calls: true,
      night_mode_strength: 40,
      night_mode_slope: 10,
      graphql_endpoint_enabled: false,
      enable_push_to_talk_shelf: true,
      non_spotify_playback_ios: true,
      graphql_for_shelf_enabled: false,
      sunset_info_screen: false,
      sunset_kill_switch: false,
      sunset_info_screen_nag: false,
    }
  }
}

