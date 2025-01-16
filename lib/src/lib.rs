mod shared;
pub use shared::*;

pub mod client;
pub mod gateway;
pub mod server;
pub mod stock;

pub use client::{ClientCommand, ClientCommandType};
pub use server::{ServerEvent, ServerEventData, ServerEventType};

pub const BRIDGETHING_SERVICE_UUID: uuid::Uuid = uuid::Uuid::from_u128(0xD8EF030853E54085A5D8F55F3F14AC5A);
pub const BRIDGETHING_CHARACTERISTIC_UUID: uuid::Uuid = uuid::Uuid::from_u128(0x8AABE8FAF3DC46208B748BD49BB5A468);
pub const MANUFACTURER_ID: u16 = 0xf00d;
