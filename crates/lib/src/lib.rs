mod macros;
mod shared;

pub mod client;
pub mod gateway;
pub mod stock;
pub mod wire;

#[cfg(feature = "protocol")]
pub mod protocol;

pub use shared::{
  AcceptCallAction, Album, AncsAuthState, Artist, AssetRetention, AudioCapabilities, BridgeThingMeta, BrightnessMode,
  BrightnessState, BrowseEntry, BrowseFolder, BrowseResult, CARTHING_HACKS_LOGO, CallEndReason, Capabilities,
  CommunicationsState, CompanionAuthorityScope, ConfigEntry, ConfigField, CurrentlyActiveApplication, Device,
  DeviceType, Diagnostics, DismissReason, DtmfTone, EndCallAction, FavoritesPage, ForwardMessage, GatewayCapabilities,
  GatewayInfo, GeoAccuracy, GeoError, HardwareError, HardwareState, HttpHeader, HttpMethod, IMAGE_SIZE, Image,
  InitiateCallType, ItemKind, ItemRef, LIBBRIDGETHING_VERSION, LibraryError, LibraryItem, LogEntry, LogLevel,
  LogSource, MediaItem, MediaItemUpdate, MediaType, MusicProvider, NetError, NetFetchRequest, NetFetchResponse,
  NetworkInfo, NetworkKind, Notification, NotificationAction, NotificationApp, NotificationCategory, NotificationFlags,
  NowPlayingUpdate, OtaError, OtaErrorCode, OtaKind, OtaPhase, OtaProgress, Peer, PeerCompanionStatus, PeerIap2Status,
  PhoneCall, PhoneCallDirection, PhoneCallService, PhoneCallStatus, PhoneError, PhoneState, PlayContext, Playback,
  PlaybackOptions, PlaybackQueue, PlaybackRestrictions, PlaybackState, PlaybackUpdate, PlayerError, PlayerOptions,
  PlayerState, Playlist, PodcastEpisode, Position, Priority, QueueItem, QueuePosition, RangePart, RangeSpec,
  RecommendationsResult, RegistrationStatus, RepeatMode, SearchResult, Show, ShuffleMode, Station, StreamBegin,
  StreamChunk, StreamEnd, StreamError, SurfaceAvailability, THUMBNAIL_SIZE, TimeInfo, Track, TtlRetention,
  TunnelClosed, TunnelData, TunnelError, VoiceDescriptor, WebappInfo, WebappManifest, WebappRole, WebappSource,
  WsError, WsFrame, to_slug,
};

pub const BRIDGETHING_DEVICE_CLASS: u32 = 0x7c0000;
pub const BRIDGETHING_PROFILE_UUID: uuid::Uuid = uuid::Uuid::from_u128(0xdead0000_854d_408e_81f0_fb6147f918fd);
pub const BRIDGETHING_RFCOMM_CHANNEL: u8 = 1;

pub const BRIDGETHING_STOCK_WS_PORT: u16 = 8890;
pub const BRIDGETHING_WS_MODERN_PORT: u16 = 8891;
pub const BRIDGETHING_FILE_SERVE_PORT: u16 = 8891;
pub const BRIDGETHING_NETWORK_GATEWAY_PORT: u16 = 8892;
/// Loopback HTTP-Range proxy that libswupdate's delta downloader hits
/// for `.zck` byte ranges. Bound to `127.0.0.1`; the daemon translates
/// each request into a wire `OtaAssetRange` to the pinned companion.
pub const BRIDGETHING_OTA_RANGE_PROXY_PORT: u16 = 8893;
