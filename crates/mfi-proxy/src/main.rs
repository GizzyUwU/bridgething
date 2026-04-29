//! Device-side proxy that exposes the MFi chip over TCP.
//!
//! Run on the Car Thing during dev iteration; pair with
//! [`bridgething_mfi::RemoteI2c`] on the dev host. Single-client at a
//! time - when one client disconnects the next can connect. The chip is
//! a single-resource so concurrency would be a footgun anyway.
//!
//! Usage: `bridgething-mfi-proxy [BIND] [DEVICE] [follower]`
//!
//! Defaults: `0.0.0.0:9090 /dev/i2c-3 0x10`. The follower address may be
//! passed as decimal or `0x`-prefixed hex.

use std::env;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::ExitCode;

use bridgething_mfi::{LinuxI2c, LinuxI2cConfig, serve_remote};
use tracing::{error, info};

const DEFAULT_BIND: &str = "0.0.0.0:9090";
const DEFAULT_DEVICE: &str = "/dev/i2c-3";

fn main() -> ExitCode {
  tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
    .init();

  let mut args = env::args().skip(1);
  let bind: String = args.next().unwrap_or_else(|| DEFAULT_BIND.into());
  let device: PathBuf = args
    .next()
    .map(PathBuf::from)
    .unwrap_or_else(|| PathBuf::from(DEFAULT_DEVICE));
  let follower: u16 = match args.next() {
    Some(s) => match parse_addr(&s) {
      Ok(v) => v,
      Err(e) => {
        eprintln!("invalid follower address {s:?}: {e}");
        return ExitCode::from(2);
      }
    },
    None => LinuxI2cConfig::DEFAULT_ADDRESS,
  };

  let cfg = LinuxI2cConfig::new(&device, follower);
  info!(%bind, device = %device.display(), follower = format!("0x{follower:02x}"), "bridgething-mfi-proxy starting");

  let listener = match TcpListener::bind(&bind) {
    Ok(l) => l,
    Err(e) => {
      error!(error = %e, %bind, "bind failed");
      return ExitCode::from(1);
    }
  };

  for incoming in listener.incoming() {
    let stream = match incoming {
      Ok(s) => s,
      Err(e) => {
        error!(error = %e, "accept failed");
        continue;
      }
    };
    if let Err(e) = stream.set_nodelay(true) {
      error!(error = %e, "set_nodelay failed");
    }
    let peer = stream.peer_addr().map(|a| a.to_string()).unwrap_or_else(|_| "?".into());
    info!(%peer, "client connected");

    let mut transport = match LinuxI2c::open(&cfg) {
      Ok(t) => t,
      Err(e) => {
        error!(error = %e, "failed to open i2c device for client");
        continue;
      }
    };

    match serve_remote(stream, &mut transport) {
      Ok(()) => info!(%peer, "client disconnected"),
      Err(e) => error!(error = %e, %peer, "client error"),
    }
  }

  ExitCode::SUCCESS
}

fn parse_addr(s: &str) -> Result<u16, std::num::ParseIntError> {
  if let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
    u16::from_str_radix(rest, 16)
  } else {
    s.parse()
  }
}
