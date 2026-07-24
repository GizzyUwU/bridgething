use std::time::Duration;

use crate::{
  CHALLENGE_LEN, RESPONSE_LEN, SERIAL_LEN, cmd,
  error::{Error, Result},
  transport::{CERT_SETTLE, Transport},
};

const SIGN_POLL_DELAY: Duration = Duration::from_millis(500);

pub struct MfiAuth<T: Transport> {
  transport: T,
}

impl<T: Transport> MfiAuth<T> {
  pub fn with_transport(transport: T) -> Self {
    Self { transport }
  }

  pub fn into_transport(self) -> T {
    self.transport
  }

  pub fn transport_mut(&mut self) -> &mut T {
    &mut self.transport
  }

  pub fn version(&mut self) -> Result<u8> {
    self.read_byte(cmd::VERSION)
  }

  pub fn last_error(&mut self) -> Result<u8> {
    self.read_byte(cmd::ERROR)
  }

  pub fn status(&mut self) -> Result<u8> {
    self.read_byte(cmd::STATUS)
  }

  pub fn cert_len(&mut self) -> Result<u16> {
    self.transport.prepare(cmd::CERT_LEN).map_err(Error::Transport)?;
    self.transport.sleep(CERT_SETTLE);
    let mut buf = [0u8; 2];
    self.transport.raw_read(&mut buf).map_err(Error::Transport)?;
    Ok(u16::from_be_bytes(buf))
  }

  pub fn cert_into(&mut self, out: &mut [u8]) -> Result<usize> {
    let len = usize::from(self.cert_len()?);
    if out.len() < len {
      return Err(Error::BufferTooSmall {
        need: len,
        got: out.len(),
      });
    }
    self.transport.prepare(cmd::CERT).map_err(Error::Transport)?;
    self.transport.sleep(CERT_SETTLE);
    self.transport.raw_read(&mut out[..len]).map_err(Error::Transport)?;
    Ok(len)
  }

  pub fn cert(&mut self) -> Result<Vec<u8>> {
    let len = usize::from(self.cert_len()?);
    let mut out = vec![0u8; len];
    self.transport.prepare(cmd::CERT).map_err(Error::Transport)?;
    self.transport.sleep(CERT_SETTLE);
    self.transport.raw_read(&mut out).map_err(Error::Transport)?;
    Ok(out)
  }

  pub fn serial(&mut self) -> Result<[u8; SERIAL_LEN]> {
    self.transport.prepare(cmd::SERIAL).map_err(Error::Transport)?;
    let mut out = [0u8; SERIAL_LEN];
    self.transport.raw_read(&mut out).map_err(Error::Transport)?;
    Ok(out)
  }

  pub fn sign(&mut self, challenge: &[u8; CHALLENGE_LEN]) -> Result<[u8; RESPONSE_LEN]> {
    self
      .transport
      .smbus_write_block(cmd::CHALLENGE, challenge)
      .map_err(Error::Transport)?;

    let mut len_buf = [0u8; 2];
    self
      .transport
      .smbus_read_block(cmd::CHALLENGE_LEN, &mut len_buf)
      .map_err(Error::Transport)?;
    let echoed = u16::from_be_bytes(len_buf);
    if echoed != cmd::EXPECTED_CHALLENGE_LEN {
      return Err(Error::UnexpectedChallengeLen {
        got: echoed,
        expected: cmd::EXPECTED_CHALLENGE_LEN,
      });
    }

    self
      .transport
      .smbus_write_block(cmd::START_RESPONSE, &[cmd::START_RESPONSE_TRIGGER])
      .map_err(Error::Transport)?;

    self.transport.sleep(SIGN_POLL_DELAY);

    let status = self.read_byte(cmd::STATUS)?;
    if status != cmd::STATUS_READY {
      return Err(Error::SignNotReady { status });
    }

    self.transport.prepare(cmd::RESPONSE).map_err(Error::Transport)?;
    let mut response = [0u8; RESPONSE_LEN];
    self.transport.raw_read(&mut response).map_err(Error::Transport)?;
    Ok(response)
  }

  fn read_byte(&mut self, cmd: u8) -> Result<u8> {
    let mut buf = [0u8; 1];
    self
      .transport
      .smbus_read_block(cmd, &mut buf)
      .map_err(Error::Transport)?;
    Ok(buf[0])
  }
}

#[cfg(target_os = "linux")]
impl MfiAuth<crate::LinuxI2c> {
  pub fn open_default() -> Result<Self> {
    Self::open(&crate::LinuxI2cConfig::default())
  }

  pub fn open(config: &crate::LinuxI2cConfig) -> Result<Self> {
    let t = crate::LinuxI2c::open(config).map_err(Error::Transport)?;
    Ok(Self::with_transport(t))
  }
}
