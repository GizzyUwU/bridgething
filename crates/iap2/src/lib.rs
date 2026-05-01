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
pub use session::{Iap2Session, MfiAccess, MfiHandle, SessionEvent, WorkerMfiAccess};

/// SDP service-class UUID the accessory advertises for an iAP2-over-RFCOMM
/// listener. iPhones scan for this UUID, read the channel from the
/// matching SDP record, and open RFCOMM there.
pub const IAP2_ACCESSORY_UUID: uuid::Uuid = uuid::Uuid::from_u128(0x00000000_DECA_FADE_DECA_DEAFDECACAFF);

/// SDP service-class UUID iOS devices advertise for inbound iAP2-over-RFCOMM.
/// The trailing nibble (`E` vs the accessory's `F`) is the only difference.
/// Accessories register a client-role profile against this UUID and dial it
/// when reconnecting after wake (cleanroom doc `protocol/10_rfcomm_transport.md`).
pub const IAP2_DEVICE_UUID: uuid::Uuid = uuid::Uuid::from_u128(0x00000000_DECA_FADE_DECA_DEAFDECACAFE);

/// RFCOMM channel the accessory binds for iAP2. The protocol does not
/// pin a channel; whatever we bind, we advertise. Channel 1 is taken
/// by the bridgething-native gateway, so iAP2 lives on 2.
pub const IAP2_RFCOMM_CHANNEL: u8 = 2;
