use std::io::{Cursor, Read};

use flate2::read::GzDecoder;
use tokio_util::{
  bytes::{Buf, BufMut, BytesMut},
  codec::{Decoder, Encoder},
};
use tracing;

use super::{COMPRESSION_NONE, ENCODING_MSGPACK, EndecError, EndecState, HEADER_LEN, MAGIC, TypedDecodeError, VERSION};
use crate::{
  Priority,
  gateway::{BridgeToGatewayMsg, GatewayToBridgeMsg},
  protocol::{Compression, Encoding, PrioritizedFrame, mbps, try_probe_envelope_json, try_probe_envelope_msgpack},
};

#[derive(Debug, Default)]
pub struct GatewayEndec {
  state: Option<EndecState>,
}

impl Decoder for GatewayEndec {
  type Item = PrioritizedFrame<BridgeToGatewayMsg>;
  type Error = EndecError;

  fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
    if src.is_empty() {
      return Ok(None);
    }

    let state = self.state.get_or_insert_default();

    if !state.header_parsed {
      if src.len() < HEADER_LEN {
        tracing::trace!(target: "libbridgething::protocol::gateway::decoder", "not enough bytes for header (need {}, have {})", HEADER_LEN, src.len());
        state.packet += 1;
        return Ok(None);
      }

      let magic = u16::from_be_bytes([src[0], src[1]]);
      if magic != MAGIC {
        tracing::error!(target: "libbridgething::protocol::gateway::decoder", "invalid magic: {:#x}", magic);
        // drop junk
        src.clear();
        return Err(EndecError::InvalidMagic);
      }

      state.version = src[2];
      if state.version != VERSION {
        tracing::error!(target: "libbridgething::protocol::gateway::decoder", "unsupported version: {}", state.version);
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
      state.header_parsed = true;
      tracing::trace!(target: "libbridgething::protocol::gateway::decoder", "message length {}, compression {:?}, encoding {:?}, priority {:?}", state.length, state.compression, state.encoding, state.priority);
    }

    if src.len() < state.total_length {
      tracing::trace!(target: "libbridgething::protocol::gateway::decoder", "message not complete ({}/{} bytes)", src.len(), state.total_length);
      state.packet += 1;
      return Ok(None);
    }

    src.advance(HEADER_LEN);
    let body = src.split_to(state.length as usize);

    let mut decompressed: Vec<u8> = Vec::new();
    let payload: &[u8] = if state.compression == Compression::Gzip {
      tracing::trace!(target: "libbridgething::protocol::gateway::decoder", "decompressing gzip data");
      let mut decoder = GzDecoder::new(Cursor::new(&body[..]));
      decoder.read_to_end(&mut decompressed)?;
      tracing::trace!(target: "libbridgething::protocol::gateway::decoder", "decompressed {} bytes", decompressed.len());
      &decompressed
    } else {
      tracing::trace!(target: "libbridgething::protocol::gateway::decoder", "using uncompressed data");
      &body
    };

    tracing::trace!(target: "libbridgething::protocol::gateway::decoder", "deserializing message with {} bytes", payload.len());

    if state.packet != 0 {
      let elapsed_time = state.message_start.elapsed();
      tracing::debug!(target: "libbridgething::protocol::gateway::decoder", "network bytes: {}, total bytes: {}, elapsed {:?}", state.length, payload.len(), elapsed_time);
      tracing::trace!(target: "libbridgething::protocol::gateway::decoder", "transfer rate: {:.2}mbps, effective rate: {:.2}mbps", mbps(elapsed_time, state.total_length as f64), mbps(elapsed_time, (HEADER_LEN + payload.len()) as f64));
    }

    let priority = state.priority;
    let encoding = state.encoding;
    self.state = None;

    let msg: BridgeToGatewayMsg = match encoding {
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
    tracing::trace!(target: "libbridgething::protocol::gateway::decoder", "successfully decoded message");

    Ok(Some(PrioritizedFrame { priority, msg }))
  }
}

impl Encoder<GatewayToBridgeMsg> for GatewayEndec {
  type Error = EndecError;

  fn encode(&mut self, item: GatewayToBridgeMsg, dst: &mut BytesMut) -> Result<(), Self::Error> {
    self.encode(PrioritizedFrame::normal(item), dst)
  }
}

impl Encoder<PrioritizedFrame<GatewayToBridgeMsg>> for GatewayEndec {
  type Error = EndecError;

  fn encode(&mut self, item: PrioritizedFrame<GatewayToBridgeMsg>, dst: &mut BytesMut) -> Result<(), Self::Error> {
    tracing::trace!(target: "libbridgething::protocol::gateway::encode", "serializing message");
    // rmp-serde to_vec_named keeps field-name metadata in the wire so polyglot
    // decoders (Swift / Kotlin / TS) don't depend on Rust struct field order.
    let packed = rmp_serde::to_vec_named(&item.msg).map_err(EndecError::RmpSerialization)?;
    let len = packed.len() as u64;
    tracing::trace!(target: "libbridgething::protocol::gateway::encode", "serialized to {len} bytes, priority {:?}", item.priority);

    dst.put_u16(MAGIC);
    dst.put_u8(VERSION);
    dst.put_u8(COMPRESSION_NONE);
    dst.put_u8(ENCODING_MSGPACK);
    dst.put_u8(item.priority.as_byte());
    dst.put_bytes(0, 2); // reserved
    dst.put_u64(len);

    dst.extend_from_slice(&packed);
    tracing::trace!(target: "libbridgething::protocol::gateway::encode", "message encoded successfully");
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    gateway::{BridgeToGatewayMsgData, BridgeToGatewayTransferMsg, TransferAck},
    wire::MsgMeta,
  };

  fn sample() -> BridgeToGatewayMsg {
    BridgeToGatewayMsg {
      id: uuid::Uuid::now_v7(),
      meta: MsgMeta::Event,
      data: BridgeToGatewayMsgData::Transfer(BridgeToGatewayTransferMsg::Ack(TransferAck {
        transfer_id: uuid::Uuid::now_v7(),
        received: 4096,
      })),
    }
  }

  fn frame_bytes(msg: &BridgeToGatewayMsg) -> Vec<u8> {
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

  // regression: a header straddling two reads must not be treated as a parsed
  // zero-length frame (the packet-count gating bug that desynced byte streams).
  #[test]
  fn header_split_across_reads_decodes() {
    let msg = sample();
    let bytes = frame_bytes(&msg);
    for split in 1..HEADER_LEN {
      let mut codec = GatewayEndec::default();
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
}
