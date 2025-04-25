use std::time::{Duration, Instant};

mod bridge;
mod gateway;

pub use bridge::*;
pub use gateway::*;

const HEADER_LEN: usize = 12;
const MAGIC: u16 = 0xdead;
const VERSION: u8 = 1;
const COMPRESSION_GZIP: u8 = 0x01;
const COMPRESSION_NONE: u8 = 0x00;

#[derive(Debug, Clone)]
struct EndecState {
  version: u8,
  compression: Compression,
  length: u64,

  total_length: usize,
  packet: usize,
  message_start: Instant,
}

impl Default for EndecState {
  fn default() -> Self {
    Self {
      version: VERSION,
      compression: Compression::Gzip,
      length: 0,

      total_length: 0,
      packet: 0,
      message_start: Instant::now(),
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Compression {
  Gzip,
  None,
}

impl From<u8> for Compression {
  fn from(value: u8) -> Self {
    match value {
      COMPRESSION_GZIP => Compression::Gzip,
      COMPRESSION_NONE => Compression::None,
      _ => Compression::None,
    }
  }
}

fn mbps(elapsed: Duration, length: f64) -> f64 {
  let bits = length * 8.0;
  let megabits = bits / 1_000_000.0;
  let seconds = elapsed.as_secs_f64();
  megabits / seconds
}

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
