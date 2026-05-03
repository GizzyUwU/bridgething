mod macros;
mod shared;

pub mod client;
pub mod gateway;
pub mod stock;
pub mod wire;

#[cfg(feature = "protocol")]
pub mod protocol;

pub use shared::{
  Album, Artist, AssetRetention, AudioCapabilities, BridgeThingMeta, BrightnessMode, BrightnessState, BrowseEntry,
  BrowseFolder, BrowseResult, CARTHING_HACKS_LOGO, CallEndReason, Capabilities, CompanionAuthorityScope,
  CurrentlyActiveApplication, Device, DeviceType, Diagnostics, DismissReason, FavoritesPage, ForwardMessage,
  GatewayCapabilities, GatewayInfo, GeoAccuracy, GeoError, HardwareError, HardwareState, HttpHeader, HttpMethod,
  IMAGE_SIZE, Image, ItemKind, ItemRef, LIBBRIDGETHING_VERSION, LibraryError, LibraryItem, LogEntry, LogLevel,
  LogSource, MediaItem, MediaItemUpdate, NET_FETCH_INLINE_MAX_BYTES, NetError, NetFetchRequest, NetFetchResponse,
  NetFetchStreamBegin, NetFetchStreamChunk, NetFetchStreamEnd, NetworkInfo, NetworkKind, Notification,
  NotificationAction, NotificationApp, NotificationCategory, NotificationError, NotificationFlags, NotificationsPage,
  NowPlayingUpdate, Peer, PeerCompanionStatus, PeerIap2Status, PhoneCall, PhoneCallDirection, PhoneCallStatus,
  PhoneError, PhoneState, PlayContext, Playback, PlaybackOptions, PlaybackQueue, PlaybackRestrictions, PlaybackState,
  PlaybackUpdate, PlayerError, PlayerOptions, PlayerState, Playlist, PodcastEpisode, Position, Priority, QueueItem,
  QueuePosition, RecommendationsResult, RepeatMode, SearchResult, Show, Station, SurfaceAvailability, THUMBNAIL_SIZE,
  TimeInfo, Track, TtlRetention, VoiceDescriptor, WebappInfo, WebappSource, WsError, WsFrame, to_slug,
};

pub const BRIDGETHING_DEVICE_CLASS: u32 = 0x7c0000;
pub const BRIDGETHING_PROFILE_UUID: uuid::Uuid = uuid::Uuid::from_u128(0xdead0000_854d_408e_81f0_fb6147f918fd);
pub const BRIDGETHING_RFCOMM_CHANNEL: u8 = 1;

pub const BRIDGETHING_STOCK_WS_PORT: u16 = 8890;
pub const BRIDGETHING_WS_MODERN_PORT: u16 = 8891;
pub const BRIDGETHING_FILE_SERVE_PORT: u16 = 8891;
pub const BRIDGETHING_NETWORK_GATEWAY_PORT: u16 = 8892;
