use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum StockInterAppRecv {
  #[serde(rename = "com.spotify.superbird.crashes.report")]
  CrashReport,
  #[serde(rename = "com.spotify.superbird.earcon")]
  Earcon,
  #[serde(rename = "com.spotify.get_available_podcast_playback_speeds")]
  GetAvailablePodcastPlaybackSpeeds,
  #[serde(rename = "com.spotify.get_capabilities")]
  GetCapabilities,
  #[serde(rename = "com.spotify.get_children_of_item")]
  GetChildrenOfItem,
  #[serde(rename = "com.spotify.superbird.get_home")]
  GetHome,
  #[serde(rename = "com.spotify.get_crossfade_state")]
  GetCrossfadeState,
  #[serde(rename = "com.spotify.get_current_context")]
  GetCurrentContext,
  #[serde(rename = "com.spotify.get_current_track")]
  GetCurrentTrack,
  #[serde(rename = "com.spotify.get_image")]
  GetImage,
  #[serde(rename = "com.spotify.get_items_for_uris")]
  GetItemForURI,
  #[serde(rename = "com.spotify.get_next_tracks")]
  GetNextTracks,
  #[serde(rename = "com.spotify.superbird.permissions")]
  GetPermissions,
  #[serde(rename = "com.spotify.get_playback_speed")]
  GetPlaybackSpeed,
  #[serde(rename = "com.spotify.get_player_state")]
  GetPlayerState,
  #[serde(rename = "com.spotify.superbird.get_podcast")]
  GetPodcast,
  #[serde(rename = "com.spotify.get_podcast_playback_speed")]
  GetPodcastPlaybackSpeed,
  #[serde(rename = "com.spotify.superbird.presets.get_presets")]
  GetPresets,
  #[serde(rename = "com.spotify.get_rating")]
  GetRating,
  #[serde(rename = "com.spotify.get_recommended_content_for_type")]
  GetRecommendedContentForType,
  #[serde(rename = "com.spotify.get_repeat")]
  GetRepeat,
  #[serde(rename = "com.spotify.get_root_item")]
  GetRootItem,
  #[serde(rename = "com.spotify.get_saved")]
  GetSaved,
  #[serde(rename = "com.spotify.get_session_state")]
  GetSessionState,
  #[serde(rename = "com.spotify.get_shuffle")]
  GetShuffle,
  #[serde(rename = "com.spotify.get_thumbnail_image")]
  GetThumbnailImage,
  #[serde(rename = "com.spotify.superbird.tipsandtricks.get_tips_and_tricks")]
  GetTips,
  #[serde(rename = "com.spotify.get_track_elapsed")]
  GetTrackElapsed,
  #[serde(rename = "com.spotify.superbird.tts.speak")]
  GetTts,
  #[serde(rename = "com.spotify.superbird.graphql")]
  Graph,
  #[serde(rename = "com.spotify.log_message")]
  LogMessage,
  #[serde(rename = "com.spotify.superbird.pitstop.log")]
  PitstopLog,
  #[serde(rename = "com.spotify.play_item")]
  _PlayItem,
  #[serde(rename = "com.spotify.play_uri")]
  _PlayUri,
  #[serde(rename = "com.spotify.superbird.play_podcast_trailer")]
  PlayPodcastTrailer,
  #[serde(rename = "com.spotify.queue_spotify_uri")]
  QueueUri,
  #[serde(rename = "com.spotify.search_query")]
  SearchQuery,
  #[serde(rename = "com.spotify.set_playback_position")]
  _SeekToPosition,
  #[serde(rename = "com.spotify.set_playback_speed")]
  _SetPlaybackSpeed,
  #[serde(rename = "com.spotify.set_podcast_playback_speed")]
  SetPodcastPlaybackSpeed,
  #[serde(rename = "com.spotify.superbird.presets.set_preset")]
  SetPreset,
  #[serde(rename = "com.spotify.set_rating")]
  SetRating,
  #[serde(rename = "com.spotify.set_repeat")]
  _SetRepeat,
  #[serde(rename = "com.spotify.set_saved")]
  SetSaved,
  #[serde(rename = "com.spotify.set_shuffle")]
  _SetShuffle,
  #[serde(rename = "com.spotify.skip_next")]
  _SkipNext,
  #[serde(rename = "com.spotify.skip_previous")]
  _SkipPrevious,
  #[serde(rename = "com.spotify.skip_to_index_in_queue")]
  SkipToIndex,
  #[serde(rename = "com.spotify.start_radio")]
  StartRadio,
  #[serde(rename = "com.spotify.superbird.dj.summon")]
  SummonDj,
  #[serde(rename = "com.spotify.superbird.instrumentation.request")]
  RequestLog,
  #[serde(rename = "com.spotify.superbird.instrumentation.interaction")]
  SendUbiInteraction,
  #[serde(rename = "com.spotify.superbird.instrumentation.impression")]
  SendUbiImpression,
  #[serde(rename = "com.spotify.superbird.instrumentation.log")]
  SendUbiBatch,
  #[serde(rename = "com.spotify.superbird.phone.answer")]
  PhoneAnswer,
  #[serde(rename = "com.spotify.superbird.phone.decline")]
  PhoneDecline,
  #[serde(rename = "com.spotify.superbird.phone.get_image")]
  PhoneCallImage,
  #[serde(rename = "com.spotify.superbird.phone.send_message")]
  PhoneCallMessage,
  #[serde(rename = "com.spotify.superbird.volume.volume_up")]
  IncreaseVolume,
  #[serde(rename = "com.spotify.superbird.volume.volume_down")]
  DecreaseVolume,
  #[serde(rename = "com.spotify.superbird.play_uri")]
  PlayUri,
  #[serde(rename = "com.spotify.superbird.skip_next")]
  SkipNext,
  #[serde(rename = "com.spotify.superbird.skip_prev")]
  SkipPrev,
  #[serde(rename = "com.spotify.superbird.seek_to")]
  SeekTo,
  #[serde(rename = "com.spotify.superbird.resume")]
  Resume,
  #[serde(rename = "com.spotify.superbird.pause")]
  Pause,
  #[serde(rename = "com.spotify.superbird.set_shuffle")]
  SetShuffle,
  #[serde(rename = "com.spotify.superbird.set_repeat")]
  SetRepeat,
}
