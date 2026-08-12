use bytes::Bytes;
use libbridgething::gateway::{VoiceCodec, VoiceFormat};
use opus::{Channels, Decoder};

const MAX_SAMPLES_PER_CHANNEL: usize = 5760;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VoiceDecodeError {
  #[error("{0}")]
  UnsupportedFormat(String),
  #[error("opus decode failed: {0}")]
  Decode(String),
}

pub fn decode(packets: &[Bytes], format: VoiceFormat) -> Result<Vec<f32>, VoiceDecodeError> {
  match format.codec {
    VoiceCodec::Opus => decode_opus(packets, format),
  }
}

fn decode_opus(packets: &[Bytes], format: VoiceFormat) -> Result<Vec<f32>, VoiceDecodeError> {
  if packets.is_empty() {
    return Ok(Vec::new());
  }

  let channels = match format.channels {
    1 => Channels::Mono,
    2 => Channels::Stereo,
    other => {
      return Err(VoiceDecodeError::UnsupportedFormat(format!(
        "opus has no {other}-channel mode"
      )));
    }
  };
  let mut decoder = Decoder::new(format.sample_rate_hz, channels)
    .map_err(|error| VoiceDecodeError::UnsupportedFormat(error.to_string()))?;

  let lanes = format.channels as usize;
  let mut frame = vec![0f32; MAX_SAMPLES_PER_CHANNEL * lanes];
  let mut pcm: Vec<f32> = Vec::new();
  for packet in packets {
    let samples = decoder
      .decode_float(packet, &mut frame, false)
      .map_err(|error| VoiceDecodeError::Decode(error.to_string()))?;
    if lanes == 1 {
      pcm.extend_from_slice(&frame[..samples]);
    } else {
      pcm.extend(
        frame[..samples * lanes]
          .chunks_exact(lanes)
          .map(|lane| lane.iter().sum::<f32>() / lanes as f32),
      );
    }
  }
  Ok(pcm)
}
