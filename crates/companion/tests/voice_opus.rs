#[path = "voicekit/samples.rs"]
mod samples;
mod voicekit;

use bridgething_companion::voice::opus::{self, VoiceDecodeError};
use bytes::Bytes;
use libbridgething::gateway::{VoiceCodec, VoiceFormat};

const TONE_HZ: f64 = 440.0;

fn energy(samples: &[f32], at_hz: f64) -> f64 {
  let coeff = 2.0 * (2.0 * std::f64::consts::PI * at_hz / voicekit::SAMPLE_RATE_HZ as f64).cos();
  let (mut s1, mut s2) = (0.0, 0.0);
  for sample in samples {
    let s = *sample as f64 + coeff * s1 - s2;
    s2 = s1;
    s1 = s;
  }
  s1 * s1 + s2 * s2 - coeff * s1 * s2
}

#[test]
fn a_real_decode_lands_on_the_streams_own_rate() {
  let packets = voicekit::packets();
  let samples = opus::decode(&packets, voicekit::format()).expect("the capture decodes");
  let expected = packets.len() * samples::SAMPLES_PER_PACKET;
  assert_eq!(
    samples.len(),
    expected,
    "decoded {} samples, expected {expected} at {} hz",
    samples.len(),
    voicekit::SAMPLE_RATE_HZ
  );
}

#[test]
fn the_decoded_waveform_is_the_tone_that_was_encoded_not_noise() {
  let samples = opus::decode(&voicekit::packets(), voicekit::format()).expect("the capture decodes");
  let tone = energy(&samples, TONE_HZ);
  let elsewhere = energy(&samples, 1500.0);
  assert!(
    tone > elsewhere * 100.0,
    "440 hz carried {tone}, 1500 hz carried {elsewhere}"
  );
}

#[test]
fn packets_are_decoded_in_sequence_order_however_they_arrived() {
  let ordered = opus::decode(&voicekit::packets(), voicekit::format()).expect("the capture decodes");

  let mut shuffled: std::collections::BTreeMap<u32, Bytes> = std::collections::BTreeMap::new();
  for (seq, packet) in voicekit::packets().into_iter().enumerate().rev() {
    shuffled.insert(seq as u32, packet);
  }
  let resorted: Vec<Bytes> = shuffled.into_values().collect();
  let replayed = opus::decode(&resorted, voicekit::format()).expect("the capture decodes");

  assert_eq!(
    replayed, ordered,
    "reassembly by seq has to reproduce the capture order exactly"
  );
}

#[test]
fn a_turn_with_no_packets_decodes_to_nothing() {
  assert!(
    opus::decode(&[], voicekit::format())
      .expect("an empty turn is not an error")
      .is_empty()
  );
}

#[test]
fn opus_is_a_real_saving_over_the_pcm_it_replaces() {
  let packets = voicekit::packets();
  let encoded: usize = packets.iter().map(Bytes::len).sum();
  let samples = opus::decode(&packets, voicekit::format()).expect("the capture decodes");
  assert!(
    samples.len() * 2 > encoded * 8,
    "{encoded} bytes carried {} samples",
    samples.len()
  );
}

#[test]
fn a_corrupt_packet_fails_the_turn_rather_than_yielding_noise() {
  let corrupt = vec![Bytes::from_static(&[0xff, 0x00])];
  assert!(matches!(
    opus::decode(&corrupt, voicekit::format()),
    Err(VoiceDecodeError::Decode(_))
  ));
}

#[test]
fn a_rate_opus_cannot_speak_is_refused_at_construction() {
  let format = VoiceFormat {
    codec: VoiceCodec::Opus,
    sample_rate_hz: 44_100,
    channels: 1,
  };
  assert!(matches!(
    opus::decode(&voicekit::packets(), format),
    Err(VoiceDecodeError::UnsupportedFormat(_))
  ));
}

#[test]
fn a_channel_count_opus_cannot_speak_is_refused_at_construction() {
  let format = VoiceFormat {
    codec: VoiceCodec::Opus,
    sample_rate_hz: voicekit::SAMPLE_RATE_HZ,
    channels: 3,
  };
  assert!(matches!(
    opus::decode(&voicekit::packets(), format),
    Err(VoiceDecodeError::UnsupportedFormat(_))
  ));
}
