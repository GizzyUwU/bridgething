use std::time::Duration;

use crate::error::TransportError;

#[cfg(target_os = "linux")]
mod linux;

pub mod mock;
pub mod remote;

#[cfg(target_os = "linux")]
pub use linux::{LinuxI2c, LinuxI2cConfig};

pub(crate) const RETRY_LIMIT: u8 = 3;
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) const RETRY_DELAY: Duration = Duration::from_micros(860);
pub(crate) const CERT_SETTLE: Duration = Duration::from_millis(10);

pub trait Transport {
  fn prepare(&mut self, cmd: u8) -> Result<(), TransportError>;

  fn smbus_read_block(&mut self, cmd: u8, out: &mut [u8]) -> Result<(), TransportError>;

  fn smbus_write_block(&mut self, cmd: u8, data: &[u8]) -> Result<(), TransportError>;

  fn raw_read(&mut self, out: &mut [u8]) -> Result<(), TransportError>;

  fn sleep(&mut self, dur: Duration) {
    std::thread::sleep(dur);
  }
}
