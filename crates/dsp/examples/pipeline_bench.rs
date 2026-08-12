use std::{env, time::Instant};

use bridgething_dsp::{geometry::CHANNELS, pipeline::Beamformer, scene};

const SAMPLE_RATE: usize = 16_000;
const PERIOD_FRAMES: usize = 256;

fn main() {
  let seconds: f64 = env::args()
    .nth(1)
    .map(|s| s.parse().expect("bad duration"))
    .unwrap_or(60.0);
  let frames = (seconds * SAMPLE_RATE as f64) as usize;

  let mut state = 0x2545_f491_4f6c_dd1du64;
  let mut noise = move || {
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    (state >> 33) as i32 - (1 << 30)
  };
  let audio: Vec<i32> = (0..frames * CHANNELS)
    .map(|n| {
      let t = (n / CHANNELS) as f64 / SAMPLE_RATE as f64;
      let tone = ((std::f64::consts::TAU * 220.0 * t).sin() * 3.0e8) as i32;
      tone.wrapping_add(noise() / 8)
    })
    .collect();

  for (label, adaptation) in [
    ("adaptation off", None),
    ("adaptation on", Some(scene::Config::default())),
  ] {
    let mut beamformer = Beamformer::new(bridgething_dsp::pipeline::Config {
      adaptation,
      ..Default::default()
    });
    let mut out = Vec::with_capacity(PERIOD_FRAMES);

    let period = PERIOD_FRAMES * CHANNELS;
    let start = Instant::now();
    for block in audio.chunks(period) {
      beamformer.process(block, &mut out);
      out.clear();
    }
    let elapsed = start.elapsed().as_secs_f64();
    println!(
      "{label:<16} {seconds:.0}s audio, compute {elapsed:.2}s, real-time factor {:.4}  ({:.1}% of one core)",
      elapsed / seconds,
      elapsed / seconds * 100.0
    );
  }
}
