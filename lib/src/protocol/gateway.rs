use std::io::{Cursor, Read, Write};

use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use rmp_serde;
use tokio_util::{
  bytes::{Buf, BufMut, BytesMut},
  codec::{Decoder, Encoder},
};
use tracing;

use crate::gateway::{BridgeToGatewayMsg, GatewayToBridgeMsg};

use super::{EndecError, COMPRESSION_GZIP, HEADER_LEN, MAGIC, VERSION};

#[derive(Debug)]
pub struct GatewayEndec;

impl Decoder for GatewayEndec {
  type Item = BridgeToGatewayMsg;
  type Error = EndecError;

  fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
    tracing::trace!(target: "libbridgething::protocol::gateway::decode", "checking buffer with {} bytes", src.len());
    if src.len() < HEADER_LEN {
      tracing::trace!(target: "libbridgething::protocol::gateway::decode", "not enough bytes for header (need {}, have {})", HEADER_LEN, src.len());
      return Ok(None);
    }

    let magic = u16::from_be_bytes([src[0], src[1]]);
    if magic != MAGIC {
      tracing::error!(target: "libbridgething::protocol::gateway::decode", "invalid magic: {:#x}", magic);
      return Err(EndecError::InvalidMagic);
    }
    let version = src[2];
    if version != VERSION {
      tracing::error!(target: "libbridgething::protocol::gateway::decode", "unsupported version: {}", version);
      return Err(EndecError::UnsupportedVersion(version));
    }
    let comp = src[3];
    let len = u64::from_be_bytes(src[4..12].try_into().unwrap()) as usize;
    tracing::trace!(target: "libbridgething::protocol::gateway::decode", "message length {}, compression {}", len, comp);
    if src.len() < HEADER_LEN + len {
      tracing::trace!(target: "libbridgething::protocol::gateway::decode", "not enough bytes for full message (need {}, have {})", HEADER_LEN + len, src.len());
      return Ok(None);
    }
    src.advance(HEADER_LEN);
    let data = src.split_to(len).to_vec();
    let payload = if comp == COMPRESSION_GZIP {
      tracing::trace!(target: "libbridgething::protocol::gateway::decode", "decompressing gzip data");
      let mut decoder = GzDecoder::new(Cursor::new(data));
      let mut buf = Vec::new();
      decoder.read_to_end(&mut buf)?;
      tracing::trace!(target: "libbridgething::protocol::gateway::decode", "decompressed {} bytes", buf.len());
      buf
    } else {
      tracing::trace!(target: "libbridgething::protocol::gateway::decode", "using uncompressed data");
      data
    };
    tracing::trace!(target: "libbridgething::protocol::gateway::decoder", "deserializing message {:?}", payload);
    let msg: Self::Item = rmp_serde::from_slice(&payload).map_err(EndecError::RmpDeserialization)?;
    // let msg: Self::Item = serde_json::from_slice(&payload).map_err(EndecError::Json)?;
    tracing::trace!(target: "libbridgething::protocol::gateway::decode", "successfully decoded message");
    Ok(Some(msg))
  }
}

impl Encoder<GatewayToBridgeMsg> for GatewayEndec {
  type Error = EndecError;

  fn encode(&mut self, item: GatewayToBridgeMsg, dst: &mut BytesMut) -> Result<(), Self::Error> {
    tracing::trace!(target: "libbridgething::protocol::gateway::encode", "serializing message");
    // https://github.com/3Hren/msgpack-rust/issues/250
    let packed = rmp_serde::to_vec_named(&item).map_err(EndecError::RmpSerialization)?;
    // let packed = serde_json::to_vec(&item).map_err(EndecError::Json)?;
    tracing::trace!(target: "libbridgething::protocol::gateway::encode", "serialized to {} bytes", packed.len());
    tracing::trace!(target: "libbridgething::protocol::gateway::encode", "serialized {packed:?}");

    tracing::trace!(target: "libbridgething::protocol::gateway::encode", "compressing with gzip");
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&packed)?;
    let compressed = encoder.finish()?;
    let len = compressed.len() as u64;
    tracing::trace!(target: "libbridgething::protocol::gateway::encode", "compressed to {} bytes", len);

    dst.put_u16(MAGIC);
    dst.put_u8(VERSION);
    dst.put_u8(COMPRESSION_GZIP);
    dst.put_u64(len);

    dst.extend_from_slice(&compressed);
    tracing::trace!(target: "libbridgething::protocol::gateway::encode", "message encoded successfully");
    Ok(())
  }
}
