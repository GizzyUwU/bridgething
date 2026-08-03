use bytes::{Bytes, BytesMut};

use super::{CaptureFormat, MicError};

const FRAME_MS: u32 = 20;
const COMPLEXITY: i32 = 5;
const BITRATE_BPS: i32 = 24_000;
const MAX_PACKET: usize = 512;

pub(super) struct Encoder {
  inner: opus::Encoder,
  pending: BytesMut,
  frame_bytes: usize,
  scratch: Vec<i16>,
  out: Vec<u8>,
}

impl Encoder {
  pub fn new(format: CaptureFormat) -> Result<Self, MicError> {
    let channels = match format.channels {
      1 => opus::Channels::Mono,
      2 => opus::Channels::Stereo,
      other => return Err(MicError::Encode(format!("opus has no {other}-channel mode"))),
    };
    let mut inner = opus::Encoder::new(format.sample_rate_hz, channels, opus::Application::Voip)
      .map_err(|e| MicError::Encode(e.to_string()))?;
    inner
      .set_bitrate(opus::Bitrate::Bits(BITRATE_BPS))
      .map_err(|e| MicError::Encode(e.to_string()))?;
    inner
      .set_complexity(COMPLEXITY)
      .map_err(|e| MicError::Encode(e.to_string()))?;

    let frame_samples = frame_samples(&format);
    Ok(Self {
      inner,
      pending: BytesMut::with_capacity(frame_samples * 2 * 2),
      frame_bytes: frame_samples * format.channels as usize * 2,
      scratch: vec![0; frame_samples * format.channels as usize],
      out: vec![0; MAX_PACKET],
    })
  }

  pub fn push(&mut self, pcm: &[u8], packets: &mut Vec<Bytes>) -> Result<(), MicError> {
    self.pending.extend_from_slice(pcm);
    while self.pending.len() >= self.frame_bytes {
      let frame = self.pending.split_to(self.frame_bytes);
      packets.push(self.encode(&frame)?);
    }
    Ok(())
  }

  pub fn flush(&mut self) -> Result<Option<Bytes>, MicError> {
    if self.pending.is_empty() {
      return Ok(None);
    }
    let mut frame = std::mem::take(&mut self.pending);
    frame.resize(self.frame_bytes, 0);
    self.encode(&frame).map(Some)
  }

  fn encode(&mut self, frame: &[u8]) -> Result<Bytes, MicError> {
    for (sample, bytes) in self.scratch.iter_mut().zip(frame.chunks_exact(2)) {
      *sample = i16::from_le_bytes([bytes[0], bytes[1]]);
    }
    let len = self
      .inner
      .encode(&self.scratch, &mut self.out)
      .map_err(|e| MicError::Encode(e.to_string()))?;
    Ok(Bytes::copy_from_slice(&self.out[..len]))
  }
}

fn frame_samples(format: &CaptureFormat) -> usize {
  (format.sample_rate_hz * FRAME_MS / 1000) as usize
}

#[cfg(test)]
mod tests {
  use super::*;

  fn encoder() -> Encoder {
    Encoder::new(CaptureFormat::default()).expect("16k mono is a valid opus configuration")
  }

  fn chunk(samples: usize) -> Vec<u8> {
    (0..samples)
      .flat_map(|i| {
        let phase = i as f32 / 16_000.0 * 440.0 * std::f32::consts::TAU;
        ((phase.sin() * 12_000.0) as i16).to_le_bytes()
      })
      .collect()
  }

  #[test]
  fn a_twenty_millisecond_frame_is_three_hundred_twenty_samples_at_sixteen_kilohertz() {
    assert_eq!(frame_samples(&CaptureFormat::default()), 320);
  }

  #[test]
  fn a_single_capture_chunk_is_short_of_a_frame_and_emits_nothing() {
    let mut enc = encoder();
    let mut packets = Vec::new();
    enc.push(&chunk(256), &mut packets).unwrap();
    assert!(packets.is_empty(), "256 samples cannot fill a 320 sample frame");
  }

  #[test]
  fn capture_chunks_rechunk_into_whole_opus_frames() {
    let mut enc = encoder();
    let mut packets = Vec::new();
    for _ in 0..5 {
      enc.push(&chunk(256), &mut packets).unwrap();
    }
    assert_eq!(packets.len(), 4);
    assert!(packets.iter().all(|p| !p.is_empty()));
    assert_eq!(
      enc.flush().unwrap(),
      None,
      "the chunks divided evenly, nothing is left over"
    );
  }

  #[test]
  fn the_cadence_is_one_packet_per_twenty_milliseconds_of_audio() {
    let mut enc = encoder();
    let mut packets = Vec::new();
    for _ in 0..62 {
      enc.push(&chunk(256), &mut packets).unwrap();
    }
    assert_eq!(packets.len(), 49, "15872 samples is 49 whole frames with 192 left over");
  }

  #[test]
  fn a_partial_tail_is_padded_out_rather_than_dropped() {
    let mut enc = encoder();
    let mut packets = Vec::new();
    enc.push(&chunk(100), &mut packets).unwrap();
    assert!(packets.is_empty());
    let tail = enc.flush().unwrap().expect("the leftover samples still ship");
    assert!(!tail.is_empty());
    assert_eq!(
      enc.flush().unwrap(),
      None,
      "flushing twice does not invent a second packet"
    );
  }

  #[test]
  fn a_voice_packet_fits_the_degraded_link_budget() {
    let mut enc = encoder();
    let mut packets = Vec::new();
    for _ in 0..50 {
      enc.push(&chunk(320), &mut packets).unwrap();
    }
    let bytes: usize = packets.iter().map(|p| p.len()).sum();
    assert!(bytes < 12_000, "one second of opus was {bytes} bytes");
  }

  #[test]
  fn a_round_trip_through_the_codec_keeps_the_waveform() {
    let mut enc = encoder();
    let mut packets = Vec::new();
    let pcm = chunk(16_000);
    enc.push(&pcm, &mut packets).unwrap();

    let mut dec = opus::Decoder::new(16_000, opus::Channels::Mono).unwrap();
    let mut decoded: Vec<i16> = Vec::with_capacity(16_000);
    let mut frame = vec![0i16; 320];
    for packet in &packets {
      let n = dec.decode(packet, &mut frame, false).unwrap();
      decoded.extend_from_slice(&frame[..n]);
    }

    let source: Vec<i16> = pcm.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
    assert_eq!(decoded.len(), packets.len() * 320);
    assert!(
      correlation(&source, &decoded) > 0.9,
      "opus is lossy but the decoded tone has to track the source"
    );
  }

  fn correlation(a: &[i16], b: &[i16]) -> f32 {
    let window = a.len().min(b.len()) - 200;
    (0..200)
      .map(|lag| {
        let x = &a[..window];
        let y = &b[lag..lag + window];
        let dot: f64 = x.iter().zip(y).map(|(l, r)| *l as f64 * *r as f64).sum();
        let na: f64 = x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>().sqrt();
        let nb: f64 = y.iter().map(|v| (*v as f64).powi(2)).sum::<f64>().sqrt();
        (dot / (na * nb)) as f32
      })
      .fold(f32::MIN, f32::max)
  }
}
