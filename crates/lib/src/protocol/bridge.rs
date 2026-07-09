use std::io::{Cursor, Read};

use flate2::read::GzDecoder;
use tokio_util::{
  bytes::{Buf, BufMut, Bytes, BytesMut},
  codec::{Decoder, Encoder},
};

use super::{
  COMPRESSION_NONE, ENCODING_MSGPACK, EndecError, EndecState, HEADER_LEN, MAGIC, MAX_FRAME_LEN, TypedDecodeError,
  VERSION,
};
use crate::{
  Priority,
  gateway::{BridgeToGatewayMsg, GatewayToBridgeMsg},
  protocol::{Compression, Encoding, PrioritizedFrame, mbps, try_probe_envelope_json, try_probe_envelope_msgpack},
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

  let mut decompressed: Vec<u8> = Vec::new();
  let payload: &[u8] = if compression == Compression::Gzip {
    let mut decoder = GzDecoder::new(Cursor::new(&body[..]));
    decoder.read_to_end(&mut decompressed)?;
    &decompressed
  } else {
    &body
  };

  let msg: GatewayToBridgeMsg = match encoding {
    Encoding::Msgpack => match rmp_serde::from_slice(payload) {
      Ok(m) => m,
      Err(err) => {
        return Err(EndecError::TypedDecode {
          error: TypedDecodeError::Rmp(err),
          probe: Box::new(try_probe_envelope_msgpack(payload)),
        });
      }
    },
    Encoding::Json => match serde_json::from_slice(payload) {
      Ok(m) => m,
      Err(err) => {
        return Err(EndecError::TypedDecode {
          error: TypedDecodeError::Json(err),
          probe: Box::new(try_probe_envelope_json(payload)),
        });
      }
    },
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
    loop {
      if src.is_empty() {
        return Ok(None);
      }

      let state = self.state.get_or_insert_default();

      if !state.header_parsed {
        if src.len() < HEADER_LEN {
          tracing::trace!(target: "libbridgething::protocol::bridge::decoder", "not enough bytes for header (need {}, have {})", HEADER_LEN, src.len());
          state.packet += 1;
          return Ok(None);
        }

        let magic = u16::from_be_bytes([src[0], src[1]]);
        if magic != MAGIC {
          self.state = None;
          if resync_to_magic(src) {
            tracing::warn!(target: "libbridgething::protocol::bridge::decoder", "invalid magic {magic:#x}; resynced to next frame");
            continue;
          }
          return Ok(None);
        }

        let version = src[2];
        if version != VERSION {
          tracing::warn!(target: "libbridgething::protocol::bridge::decoder", "unsupported version {version}; resyncing");
          self.state = None;
          src.advance(1);
          if resync_to_magic(src) {
            continue;
          }
          return Ok(None);
        }

        let length = u64::from_be_bytes(src[8..16].try_into().unwrap()) as usize;
        if length > MAX_FRAME_LEN {
          tracing::warn!(target: "libbridgething::protocol::bridge::decoder", "frame length {length} over cap; resyncing");
          self.state = None;
          src.advance(1);
          if resync_to_magic(src) {
            continue;
          }
          return Ok(None);
        }

        state.version = version;
        state.compression = src[3].into();
        state.encoding = src[4].into();
        state.priority = Priority::from_byte(src[5]);
        // src[6..8] reserved
        state.length = length as u64;
        state.total_length = HEADER_LEN + length;
        state.header_parsed = true;
        tracing::trace!(target: "libbridgething::protocol::bridge::decoder", "message length {}, compression {:?}, encoding {:?}, priority {:?}", state.length, state.compression, state.encoding, state.priority);
      }

      if src.len() < state.total_length {
        tracing::trace!(target: "libbridgething::protocol::bridge::decoder", "message not complete ({}/{} bytes)", src.len(),state.total_length);
        state.packet += 1;
        return Ok(None);
      }

      return self.finish_frame(src);
    }
  }
}

fn resync_to_magic(src: &mut BytesMut) -> bool {
  let magic = MAGIC.to_be_bytes();
  if let Some(pos) = src.windows(magic.len()).position(|w| w == magic) {
    src.advance(pos);
    true
  } else {
    let drop = src.len().saturating_sub(magic.len() - 1);
    src.advance(drop);
    false
  }
}

impl BridgeEndec {
  fn finish_frame(&mut self, src: &mut BytesMut) -> Result<Option<PrioritizedFrame<GatewayToBridgeMsg>>, EndecError> {
    let state = self.state.as_ref().expect("finish_frame called with a populated state");
    src.advance(HEADER_LEN);
    let body = src.split_to(state.length as usize);

    let mut decompressed: Vec<u8> = Vec::new();
    let payload: &[u8] = if state.compression == Compression::Gzip {
      tracing::trace!(target: "libbridgething::protocol::bridge::decoder", "decompressing gzip data");
      let mut decoder = GzDecoder::new(Cursor::new(&body[..]));
      decoder.read_to_end(&mut decompressed)?;
      tracing::trace!(target: "libbridgething::protocol::bridge::decoder", "decompressed {} bytes", decompressed.len());
      &decompressed
    } else {
      tracing::trace!(target: "libbridgething::protocol::bridge::decoder", "using uncompressed data");
      &body
    };

    tracing::trace!(target: "libbridgething::protocol::bridge::decoder", "deserializing message with {} bytes", payload.len());

    if state.packet != 0 {
      let elapsed_time = state.message_start.elapsed();
      tracing::debug!(target: "libbridgething::protocol::bridge::decoder", "network bytes: {}, total bytes: {}, elapsed {:?}", state.length, payload.len(), elapsed_time);
      tracing::trace!(target: "libbridgething::protocol::bridge::decoder", "transfer rate: {:.2}mbps, effective rate: {:.2}mbps", mbps(elapsed_time, state.total_length as f64), mbps(elapsed_time, (HEADER_LEN + payload.len()) as f64));
    }

    let priority = state.priority;
    let encoding = state.encoding;
    // Reset before typed decode: a failed decode leaves the byte stream
    // in sync (body bytes already consumed via advance + split_to), so
    // the next decode call must parse a fresh header.
    self.state = None;

    let msg: GatewayToBridgeMsg = match encoding {
      Encoding::Msgpack => match rmp_serde::from_slice(payload) {
        Ok(m) => m,
        Err(err) => {
          return Err(EndecError::TypedDecode {
            error: TypedDecodeError::Rmp(err),
            probe: Box::new(try_probe_envelope_msgpack(payload)),
          });
        }
      },
      Encoding::Json => match serde_json::from_slice(payload) {
        Ok(m) => m,
        Err(err) => {
          return Err(EndecError::TypedDecode {
            error: TypedDecodeError::Json(err),
            probe: Box::new(try_probe_envelope_json(payload)),
          });
        }
      },
    };
    tracing::trace!(target: "libbridgething::protocol::bridge::decoder", "successfully decoded message");

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

#[cfg(test)]
mod tests {
  use uuid::Uuid;

  use super::{
    super::{COMPRESSION_NONE, ENCODING_MSGPACK, MAGIC, VERSION},
    *,
  };
  use crate::{
    gateway::{AssetNotFoundReply, GatewayToBridgeAssetMsg, GatewayToBridgeMsg, GatewayToBridgeMsgData},
    wire::MsgMeta,
  };

  fn sample(asset_id: &str) -> GatewayToBridgeMsg {
    GatewayToBridgeMsg {
      id: Uuid::now_v7(),
      meta: MsgMeta::Request,
      data: GatewayToBridgeMsgData::Asset(GatewayToBridgeAssetMsg::NotFound(AssetNotFoundReply {
        id: asset_id.into(),
      })),
    }
  }

  fn frame_bytes(msg: &GatewayToBridgeMsg) -> Vec<u8> {
    let body = rmp_serde::to_vec_named(msg).unwrap();
    let mut out = BytesMut::new();
    out.put_u16(MAGIC);
    out.put_u8(VERSION);
    out.put_u8(COMPRESSION_NONE);
    out.put_u8(ENCODING_MSGPACK);
    out.put_u8(Priority::Normal.as_byte());
    out.put_bytes(0, 2);
    out.put_u64(body.len() as u64);
    out.extend_from_slice(&body);
    out.to_vec()
  }

  #[test]
  fn decodes_back_to_back_frames() {
    let mut codec = BridgeEndec::default();
    let (a, b) = (sample("art/a"), sample("art/b"));
    let mut buf = BytesMut::new();
    buf.extend_from_slice(&frame_bytes(&a));
    buf.extend_from_slice(&frame_bytes(&b));
    assert_eq!(codec.decode(&mut buf).unwrap().expect("first").msg.id, a.id);
    assert_eq!(codec.decode(&mut buf).unwrap().expect("second").msg.id, b.id);
    assert!(codec.decode(&mut buf).unwrap().is_none());
  }

  // regression: a header straddling two reads must not be treated as a parsed
  // zero-length frame. the old packet-count gating consumed 16 bytes as a bogus
  // empty frame and desynced the stream, silently losing one real frame per
  // unlucky read boundary on byte-stream transports (the EA-link corruption).
  #[test]
  fn header_split_across_reads_decodes() {
    let msg = sample("art/split");
    let bytes = frame_bytes(&msg);
    for split in 1..HEADER_LEN {
      let mut codec = BridgeEndec::default();
      let mut buf = BytesMut::new();
      buf.extend_from_slice(&bytes[..split]);
      assert!(
        codec.decode(&mut buf).unwrap().is_none(),
        "split {split}: partial header must yield no frame"
      );
      buf.extend_from_slice(&bytes[split..]);
      let frame = codec
        .decode(&mut buf)
        .unwrap_or_else(|e| panic!("split {split}: decode errored: {e:?}"))
        .unwrap_or_else(|| panic!("split {split}: frame lost"));
      assert_eq!(frame.msg.id, msg.id, "split {split}");
      assert!(buf.is_empty(), "split {split}: no residue");
    }
  }

  #[test]
  fn byte_at_a_time_stream_decodes() {
    let mut codec = BridgeEndec::default();
    let (a, b) = (sample("art/one"), sample("art/two"));
    let mut stream = frame_bytes(&a);
    stream.extend_from_slice(&frame_bytes(&b));
    let mut buf = BytesMut::new();
    let mut decoded = Vec::new();
    for byte in stream {
      buf.extend_from_slice(&[byte]);
      while let Some(frame) = codec.decode(&mut buf).unwrap() {
        decoded.push(frame.msg.id);
      }
    }
    assert_eq!(decoded, vec![a.id, b.id]);
  }

  #[test]
  fn resyncs_past_leading_garbage() {
    let mut codec = BridgeEndec::default();
    let msg = sample("art/x");
    let mut buf = BytesMut::new();
    buf.extend_from_slice(&[0x01, 0x02, 0x03, 0xde, 0x00, 0xff]); // junk, incl a lone magic-hi byte
    buf.extend_from_slice(&frame_bytes(&msg));
    assert_eq!(
      codec.decode(&mut buf).unwrap().expect("frame after resync").msg.id,
      msg.id
    );
  }

  #[test]
  fn corrupt_frame_does_not_kill_the_stream() {
    // a frame whose body is not valid msgpack must not drop the connection: the next valid frame
    // still decodes. this is the bug that wedged the iAP2 EA gateway on a single bad byte.
    let mut codec = BridgeEndec::default();
    let good = sample("art/good");
    let mut buf = BytesMut::new();
    buf.put_u16(MAGIC);
    buf.put_u8(VERSION);
    buf.put_u8(COMPRESSION_NONE);
    buf.put_u8(ENCODING_MSGPACK);
    buf.put_u8(Priority::Normal.as_byte());
    buf.put_bytes(0, 2);
    buf.put_u64(3);
    buf.extend_from_slice(&[0xff, 0xff, 0xff]); // a 3-byte non-msgpack body
    buf.extend_from_slice(&frame_bytes(&good));

    let first = codec.decode(&mut buf);
    assert!(
      matches!(&first, Err(e) if e.is_recoverable()),
      "a bad body is a recoverable typed-decode error, not a stream kill: {first:?}"
    );
    assert_eq!(
      codec.decode(&mut buf).unwrap().expect("recovered frame").msg.id,
      good.id
    );
  }

  #[test]
  fn resync_to_magic_finds_next_frame_start() {
    let mut buf = BytesMut::from(&[0x11, 0x22, 0xde, 0xad, 0x99][..]);
    assert!(resync_to_magic(&mut buf));
    assert_eq!(&buf[..], &[0xde, 0xad, 0x99]);
  }

  #[test]
  fn resync_to_magic_keeps_tail_when_absent() {
    let mut buf = BytesMut::from(&[0x11, 0x22, 0x33, 0xde][..]);
    assert!(!resync_to_magic(&mut buf));
    assert_eq!(
      &buf[..],
      &[0xde],
      "keeps a trailing byte for a magic that straddles reads"
    );
  }
}
