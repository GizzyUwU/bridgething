mod shared;
pub use shared::*;

pub mod client;
pub mod gateway;
pub mod server;
pub mod stock;

pub use client::{ClientCommand, ClientCommandType};
pub use server::{ServerEvent, ServerEventData, ServerEventType};

pub const BRIDGETHING_PROFILE_UUID: uuid::Uuid = uuid::Uuid::from_u128(0xdead0000_854d_408e_81f0_fb6147f918fd);
pub const BRIDGETHING_SERVICE_UUID: uuid::Uuid = uuid::Uuid::from_u128(0xdead0000_53e5_4085_a5d8_f55f3f14ac5a);
pub const BRIDGETHING_CHARACTERISTIC_UUID: uuid::Uuid = uuid::Uuid::from_u128(0xdead0000_f3dc_4620_8b74_8bd49bb5a468);
pub const BRIDGETHING_MANUFACTURER_ID: u16 = 0xdead;
