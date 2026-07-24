mod auth;
mod cmd;
mod error;
mod transport;

pub use auth::MfiAuth;
pub use error::{Error, Result, TransportError};
#[cfg(target_os = "linux")]
pub use transport::{LinuxI2c, LinuxI2cConfig};
pub use transport::{
  Transport,
  mock::{MockTransport, MockTransportState},
  remote::{RemoteI2c, serve as serve_remote},
};

pub const CHALLENGE_LEN: usize = 32;
pub const RESPONSE_LEN: usize = 64;
pub const SERIAL_LEN: usize = 32;
