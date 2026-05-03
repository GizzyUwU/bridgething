mod macros;
mod shared;

pub mod client;
pub mod gateway;
pub mod stock;
pub mod wire;

#[cfg(feature = "protocol")]
pub mod protocol;

pub use shared::{
  Album, Artist, AssetRetention, BridgeThingMeta, CARTHING_HACKS_LOGO, CurrentlyActiveApplication, Device, DeviceType,
  ForwardMessage, GatewayMeta, IMAGE_SIZE, Image, LIBBRIDGETHING_VERSION, MediaItemUpdate, NowPlayingUpdate, Peer,
  PeerCompanionStatus, PeerIap2Status, PhoneCallDirection, PhoneCallStatus, PlaybackOptions, PlaybackQueue,
  PlaybackRestrictions, PlaybackUpdate, Priority, RepeatMode, THUMBNAIL_SIZE, Track, TtlRetention, WebappInfo,
  WebappSource, to_slug,
};

pub const BRIDGETHING_DEVICE_CLASS: u32 = 0x7c0000;
pub const BRIDGETHING_PROFILE_UUID: uuid::Uuid = uuid::Uuid::from_u128(0xdead0000_854d_408e_81f0_fb6147f918fd);
pub const BRIDGETHING_RFCOMM_CHANNEL: u8 = 1;

pub const BRIDGETHING_STOCK_WS_PORT: u16 = 8890;
pub const BRIDGETHING_WS_MODERN_PORT: u16 = 8891;
pub const BRIDGETHING_FILE_SERVE_PORT: u16 = 8891;
pub const BRIDGETHING_NETWORK_GATEWAY_PORT: u16 = 8892;
