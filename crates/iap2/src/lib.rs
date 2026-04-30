//! iAP2 protocol stack for bridgething.
//!
//! This crate is transport-agnostic: it consumes any
//! `AsyncRead + AsyncWrite + Unpin` byte stream (in production this is the
//! RFCOMM socket BlueZ hands the daemon when an iPhone connects). The
//! crate is also runtime-agnostic with respect to the MFi coprocessor:
//! it consumes anything that satisfies the [`bridgething_mfi::Transport`]
//! trait, so tests can run against `MockTransport` and dev iteration can
//! run against `RemoteI2c` while production runs against `LinuxI2c`.
//!
//! The current scope is the link layer wedge: detect handshake, SYN/SYN-ACK
//! negotiation, transition to Established. Authentication, identification,
//! and steady-state CSM dispatch land in subsequent slices.

pub mod csm;
mod error;
mod frame;
mod link;
pub mod session;

pub use error::{Error, Result};
pub use frame::{
  ControlBits, DETECT_MARKER, LINK_HEADER_LEN, LINK_MAGIC, LinkCodec, LinkHeader, LinkPacket, Lsp, SessionTriple,
  SessionType,
};
pub use link::{Iap2Command, Iap2Event, Link, LinkConfig};
pub use session::{Iap2Session, MfiAccess, SessionEvent, WorkerMfiAccess};
