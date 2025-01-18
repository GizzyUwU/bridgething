mod shared;
pub use shared::*;

pub mod client;
pub mod gateway;
pub mod server;
pub mod stock;

pub use client::{ClientCommand, ClientCommandType};
pub use server::{ServerEvent, ServerEventData, ServerEventType};

pub const BRIDGETHING_SERVICE_UUID: uuid::Uuid = uuid::Uuid::from_u128(0xd8ef0308_53e5_4085_a5d8_f55f3f14ac5a);
pub const BRIDGETHING_CHARACTERISTIC_UUID: uuid::Uuid = uuid::Uuid::from_u128(0x8aabe8fa_f3dc_4620_8b74_8bd49bb5a468);
pub const MANUFACTURER_ID: u16 = 0xf00d;
