use std::io::{Cursor, Read, Write};

use flate2::{read::GzDecoder, write::GzEncoder};
use tokio_util::{
  bytes::{Buf, BufMut, BytesMut},
  codec::{Decoder, Encoder},
};

use super::{COMPRESSION_GZIP, ENCODING_MSGPACK, EndecError, EndecState, HEADER_LEN, MAGIC, VERSION};
use crate::{
  Priority,
  gateway::{BridgeToGatewayMsg, GatewayToBridgeMsg},
  protocol::{Compression, Encoding, PrioritizedFrame, mbps},
};

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
    self.encode(PrioritizedFrame::normal(item), dst)
  }
}

impl Encoder<PrioritizedFrame<BridgeToGatewayMsg>> for BridgeEndec {
  type Error = EndecError;

  fn encode(&mut self, item: PrioritizedFrame<BridgeToGatewayMsg>, dst: &mut BytesMut) -> Result<(), Self::Error> {
    tracing::trace!(target: "libbridgething::protocol::bridge::encode", "serializing message");
    // rmp-serde to_vec_named keeps field-name metadata in the wire so polyglot
    // decoders (Swift / Kotlin / TS) don't depend on Rust struct field order.
    let packed = rmp_serde::to_vec_named(&item.msg).map_err(EndecError::RmpSerialization)?;
    tracing::trace!(target: "libbridgething::protocol::bridge::encode", "serialized to {} bytes", packed.len());

    tracing::trace!(target: "libbridgething::protocol::bridge::encode", "compressing with gzip");
    let mut encoder = GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&packed)?;
    let compressed = encoder.finish()?;
    let len = compressed.len() as u64;
    tracing::trace!(target: "libbridgething::protocol::bridge::encode", "compressed to {} bytes, priority {:?}", len, item.priority);

    dst.put_u16(MAGIC);
    dst.put_u8(VERSION);
    dst.put_u8(COMPRESSION_GZIP);
    dst.put_u8(ENCODING_MSGPACK);
    dst.put_u8(item.priority.as_byte());
    dst.put_bytes(0, 2); // reserved
    dst.put_u64(len);

    dst.extend_from_slice(&compressed);
    tracing::trace!(target: "libbridgething::protocol::bridge::encode", "message encoded successfully");
    Ok(())
  }
}
