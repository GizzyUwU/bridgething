mod bridge;
mod gateway;

pub use bridge::*;
pub use gateway::*;

const HEADER_LEN: usize = 12;
const MAGIC: u16 = 0xdead;
const VERSION: u8 = 1;
const COMPRESSION_GZIP: u8 = 0x01;
#[allow(dead_code)]
const COMPRESSION_NONE: u8 = 0x00;

#[derive(Debug, thiserror::Error)]
pub enum EndecError {
  #[error("invalid magic number")]
  InvalidMagic,
  #[error("unsupported version: {0}")]
  UnsupportedVersion(u8),
  #[error("serialization error: {0}")]
  RmpSerialization(#[from] rmp_serde::encode::Error),
  #[error("deserialization error: {0}")]
  RmpDeserialization(#[from] rmp_serde::decode::Error),
  #[error("ser/de error: {0}")]
  Json(#[from] serde_json::Error),
  #[error(transparent)]
  Io(#[from] std::io::Error),
}
