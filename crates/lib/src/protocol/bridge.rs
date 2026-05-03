use std::io::{Cursor, Read};

use flate2::read::GzDecoder;
use tokio_util::{
  bytes::{Buf, BufMut, Bytes, BytesMut},
  codec::{Decoder, Encoder},
};

use super::{COMPRESSION_NONE, ENCODING_MSGPACK, EndecError, EndecState, HEADER_LEN, MAGIC, VERSION};
use crate::{
  Priority,
  gateway::{BridgeToGatewayMsg, GatewayToBridgeMsg},
  protocol::{Compression, Encoding, PrioritizedFrame, mbps},
};

/// Bytes-based decoder for one or more concatenated bridge frames.
///
/// Use this when each transport message is already a complete sequence
/// of frames (e.g. a WebSocket Binary frame produced by `OutboundPacker`)
/// and cross-message buffering isn't needed. Operates on a `Bytes` view,
/// peeling complete frames via zero-copy `split_to`. The body slice is
/// a `Bytes` view of the same allocation; msgpack decode reads it via
/// `from_slice` without an intermediate copy.
///
/// Returns `Ok(None)` when `src` doesn't yet hold a complete frame
/// (caller stitches with subsequent reads if applicable). Returns
/// `Err(EndecError::InvalidMagic)` / `UnsupportedVersion` and clears
/// `src` on framing errors so the caller can drop the connection.
pub fn parse_bridge_frame(src: &mut Bytes) -> Result<Option<PrioritizedFrame<GatewayToBridgeMsg>>, EndecError> {
  if src.len() < HEADER_LEN {
    return Ok(None);
  }

  let header = &src[..HEADER_LEN];
  let magic = u16::from_be_bytes([header[0], header[1]]);
  if magic != MAGIC {
    src.clear();
    return Err(EndecError::InvalidMagic);
  }
  let version = header[2];
  if version != VERSION {
    src.clear();
    return Err(EndecError::UnsupportedVersion(version));
  }
  let compression: Compression = header[3].into();
  let encoding: Encoding = header[4].into();
  let priority = Priority::from_byte(header[5]);
  let length = u64::from_be_bytes(header[8..16].try_into().expect("16-byte slice")) as usize;

  if src.len() < HEADER_LEN + length {
    return Ok(None);
  }

  src.advance(HEADER_LEN);
  let body = src.split_to(length);

  let msg: GatewayToBridgeMsg = if compression == Compression::Gzip {
    let mut decoder = GzDecoder::new(Cursor::new(&body[..]));
    let mut buf = Vec::new();
    decoder.read_to_end(&mut buf)?;
    match encoding {
      Encoding::Msgpack => rmp_serde::from_slice(&buf).map_err(EndecError::RmpDeserialization)?,
      Encoding::Json => serde_json::from_slice(&buf).map_err(EndecError::Json)?,
    }
  } else {
    match encoding {
      Encoding::Msgpack => rmp_serde::from_slice(&body).map_err(EndecError::RmpDeserialization)?,
      Encoding::Json => serde_json::from_slice(&body).map_err(EndecError::Json)?,
    }
  };

  Ok(Some(PrioritizedFrame { priority, msg }))
}

#[derive(Debug, Default)]
pub struct BridgeEndec {
  state: Option<EndecState>,
}

impl Decoder for BridgeEndec {
  type Item = PrioritizedFrame<GatewayToBridgeMsg>;
  type Error = EndecError;

  fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
    if src.is_empty() {
      return Ok(None);
    }

    let state = self.state.get_or_insert_default();

    if state.packet == 0 {
      if src.len() < HEADER_LEN {
        tracing::trace!(target: "libbridgething::protocol::bridge::decoder", "not enough bytes for header (need {}, have {})", HEADER_LEN, src.len());
        state.packet += 1;
        return Ok(None);
      }

      let magic = u16::from_be_bytes([src[0], src[1]]);
      if magic != MAGIC {
        tracing::error!(target: "libbridgething::protocol::bridge::decoder", "invalid magic: {:#x}", magic);
        // drop junk
        src.clear();
        return Err(EndecError::InvalidMagic);
      }

      state.version = src[2];
      if state.version != VERSION {
        tracing::error!(target: "libbridgething::protocol::bridge::decoder", "unsupported version: {}", state.version);
        // drop junk
        src.clear();
        return Err(EndecError::UnsupportedVersion(state.version));
      }

      state.compression = src[3].into();
      state.encoding = src[4].into();
      state.priority = Priority::from_byte(src[5]);
      // src[6..8] reserved
      state.length = u64::from_be_bytes(src[8..16].try_into().unwrap());
      state.total_length = HEADER_LEN + state.length as usize;
      tracing::trace!(target: "libbridgething::protocol::bridge::decoder", "message length {}, compression {:?}, encoding {:?}, priority {:?}", state.length, state.compression, state.encoding, state.priority);
    }

    if src.len() < state.total_length {
      tracing::trace!(target: "libbridgething::protocol::bridge::decoder", "message not complete ({}/{} bytes)", src.len(),state.total_length);
      state.packet += 1;
      return Ok(None);
    }

    src.advance(HEADER_LEN);
    let data = src.split_to(state.length as usize).to_vec();
    let payload = if state.compression == Compression::Gzip {
      tracing::trace!(target: "libbridgething::protocol::bridge::decoder", "decompressing gzip data");
      let mut decoder = GzDecoder::new(Cursor::new(data));
      let mut buf = Vec::new();
      decoder.read_to_end(&mut buf)?;
      tracing::trace!(target: "libbridgething::protocol::bridge::decoder", "decompressed {} bytes", buf.len());
      buf
    } else {
      tracing::trace!(target: "libbridgething::protocol::bridge::decoder", "using uncompressed data");
      data
    };

    tracing::trace!(target: "libbridgething::protocol::bridge::decoder", "deserializing message with {} bytes", payload.len());
    let msg: GatewayToBridgeMsg = match state.encoding {
      Encoding::Msgpack => rmp_serde::from_slice(&payload).map_err(EndecError::RmpDeserialization)?,
      Encoding::Json => serde_json::from_slice(&payload).map_err(EndecError::Json)?,
    };
    tracing::trace!(target: "libbridgething::protocol::bridge::decoder", "successfully decoded message");

    if state.packet != 0 {
      let elapsed_time = state.message_start.elapsed();
      tracing::debug!(target: "libbridgething::protocol::bridge::decoder", "network bytes: {}, total bytes: {}, elapsed {:?}", state.length, payload.len(), elapsed_time);
      tracing::trace!(target: "libbridgething::protocol::bridge::decoder", "transfer rate: {:.2}mbps, effective rate: {:.2}mbps", mbps(elapsed_time, state.total_length as f64), mbps(elapsed_time, (HEADER_LEN + payload.len()) as f64));
    }

    let priority = state.priority;
    self.state = None;
    Ok(Some(PrioritizedFrame { priority, msg }))
  }
}

impl Encoder<BridgeToGatewayMsg> for BridgeEndec {
  type Error = EndecError;

  fn encode(&mut self, item: BridgeToGatewayMsg, dst: &mut BytesMut) -> Result<(), Self::Error> {
    encode_bridge_frame(Priority::Normal, &item, dst)
  }
}

impl Encoder<PrioritizedFrame<BridgeToGatewayMsg>> for BridgeEndec {
  type Error = EndecError;

  fn encode(&mut self, item: PrioritizedFrame<BridgeToGatewayMsg>, dst: &mut BytesMut) -> Result<(), Self::Error> {
    encode_bridge_frame(item.priority, &item.msg, dst)
  }
}

/// Borrow-based encoder. The `Encoder` impls go through this so callers
/// holding a `&Arc<BridgeToGatewayMsg>` (the broadcast / fan-out path)
/// can reuse one allocation across every target without cloning the
/// wire payload.
pub fn encode_bridge_frame(priority: Priority, msg: &BridgeToGatewayMsg, dst: &mut BytesMut) -> Result<(), EndecError> {
  tracing::trace!(target: "libbridgething::protocol::bridge::encode", "serializing message");
  // rmp-serde to_vec_named keeps field-name metadata in the wire so polyglot
  // decoders (Swift / Kotlin / TS) don't depend on Rust struct field order.
  let packed = rmp_serde::to_vec_named(msg).map_err(EndecError::RmpSerialization)?;
  let len = packed.len() as u64;
  tracing::trace!(target: "libbridgething::protocol::bridge::encode", "serialized to {len} bytes, priority {priority:?}");

  dst.put_u16(MAGIC);
  dst.put_u8(VERSION);
  dst.put_u8(COMPRESSION_NONE);
  dst.put_u8(ENCODING_MSGPACK);
  dst.put_u8(priority.as_byte());
  dst.put_bytes(0, 2); // reserved
  dst.put_u64(len);

  dst.extend_from_slice(&packed);
  Ok(())
}
