mod shared;

pub mod client;
pub mod gateway;
pub mod server;
pub mod stock;

#[cfg(feature = "protocol")]
pub mod protocol;

pub use client::{ClientCommand, ClientCommandType};
pub use server::{ServerEvent, ServerEventData, ServerEventType};

pub use shared::{
  Album, Artist, BridgeThingMeta, CARTHING_HACKS_LOGO, CurrentlyActiveApplication, Device, DeviceType,
  ForwardMessage, GatewayMeta, IMAGE_SIZE, Image, LIBBRIDGETHING_VERSION, PhoneCallDirection, PhoneCallStatus,
  PlaybackOptions, PlaybackQueue, PlaybackRestrictions, THUMBNAIL_SIZE, Track, WebappInfo, WebappSource,
  to_slug,
};

pub const BRIDGETHING_DEVICE_CLASS: u32 = 0x7c0000;
pub const BRIDGETHING_PROFILE_UUID: uuid::Uuid = uuid::Uuid::from_u128(0xdead0000_854d_408e_81f0_fb6147f918fd);
pub const BRIDGETHING_RFCOMM_CHANNEL: u8 = 1;
pub const BRIDGETHING_SERVICE_UUID: uuid::Uuid = uuid::Uuid::from_u128(0xdead0000_53e5_4085_a5d8_f55f3f14ac5a);
pub const BRIDGETHING_CHARACTERISTIC_UUID: uuid::Uuid = uuid::Uuid::from_u128(0xdead0000_f3dc_4620_8b74_8bd49bb5a468);
pub const BRIDGETHING_MANUFACTURER_ID: u16 = 0xdead;

pub const BRIDGETHING_STOCK_WS_PORT: u16 = 8890;
pub const BRIDGETHING_WS_MODERN_PORT: u16 = 8891;
pub const BRIDGETHING_FILE_SERVE_PORT: u16 = 8891;
