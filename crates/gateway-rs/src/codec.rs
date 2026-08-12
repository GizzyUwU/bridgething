use bridgething_sdk_runtime::TransportError;
use libbridgething::{
  gateway::{BridgeToGatewayMsg, GatewayToBridgeMsg},
  protocol::{DecodedFrame, EndecError, GatewayEndec, PrioritizedFrame},
};
use tokio_util::{
  bytes::{Bytes, BytesMut},
  codec::{Decoder, Encoder},
};

pub(crate) const BATCH_BYTES: usize = 16 * 1024;

pub(crate) fn map_endec(err: EndecError) -> TransportError {
  match err {
    EndecError::Io(io) => TransportError::Io(io),
    other => TransportError::Decode(other.to_string()),
  }
}

pub(crate) fn encode_frame(frame: PrioritizedFrame<GatewayToBridgeMsg>) -> Result<Bytes, TransportError> {
  let mut dst = BytesMut::new();
  GatewayEndec::default().encode(frame, &mut dst).map_err(map_endec)?;
  Ok(dst.freeze())
}

pub(crate) fn decode_step(
  decoder: &mut GatewayEndec,
  buf: &mut BytesMut,
) -> Option<Result<BridgeToGatewayMsg, TransportError>> {
  match decoder.decode(buf) {
    Ok(Some(DecodedFrame::Frame(frame))) => Some(Ok(frame.msg)),
    Ok(Some(DecodedFrame::Failed(err))) | Err(err) => Some(Err(map_endec(err))),
    Ok(None) => None,
  }
}
