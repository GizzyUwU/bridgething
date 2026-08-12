use std::{
  fmt::{self, Debug, Display, Formatter},
  str::FromStr,
};

#[derive(Clone, Copy, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Address(pub [u8; 6]);

impl Address {
  pub const fn new(addr: [u8; 6]) -> Self {
    Self(addr)
  }
}

impl Display for Address {
  fn fmt(&self, f: &mut Formatter) -> fmt::Result {
    write!(
      f,
      "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
      self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
    )
  }
}

impl Debug for Address {
  fn fmt(&self, f: &mut Formatter) -> fmt::Result {
    write!(f, "{self}")
  }
}

impl FromStr for Address {
  type Err = InvalidAddress;

  fn from_str(s: &str) -> Result<Self, InvalidAddress> {
    let octets = s
      .split(':')
      .map(|octet| u8::from_str_radix(octet, 16).map_err(|_| InvalidAddress(s.to_string())))
      .collect::<Result<Vec<_>, InvalidAddress>>()?;

    Ok(Self(octets.try_into().map_err(|_| InvalidAddress(s.to_string()))?))
  }
}

impl From<[u8; 6]> for Address {
  fn from(addr: [u8; 6]) -> Self {
    Self(addr)
  }
}

impl From<Address> for [u8; 6] {
  fn from(addr: Address) -> Self {
    addr.0
  }
}

#[cfg(target_os = "linux")]
impl From<bluer::Address> for Address {
  fn from(addr: bluer::Address) -> Self {
    Self(addr.0)
  }
}

#[cfg(target_os = "linux")]
impl From<Address> for bluer::Address {
  fn from(addr: Address) -> Self {
    bluer::Address(addr.0)
  }
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("invalid bluetooth address: {0}")]
pub struct InvalidAddress(pub String);
