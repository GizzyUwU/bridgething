//! Scores the acoustic front end end to end against rendered multichannel scenes.
//!
//! ```text
//! cargo run --release --example scene_bench -- <scene-dir> <phrase-model> [noise-prerolls] [recent-tau] [threshold]
//! ```

use std::{collections::BTreeMap, env, path::Path};

use bridgething_dsp::{
  beamformer::Design,
  geometry::CHANNELS,
  highpass::HighPass,
  pipeline::{Beamformer, Config},
  scene,
};
use bridgething_wakeword::{WakeWord, features::CHUNK_SAMPLES};
use serde::Deserialize;

const RATE: f64 = 16_000.0;
const DEFAULT_THRESHOLD: f32 = 0.35;
const MARGIN: usize = RATE as usize;

#[derive(Deserialize)]
struct Scene {
  audio: String,
  archetype: String,
  talker_deg: f64,
  n_speakers: usize,
  snr_db: Option<f64>,
  speech_start: usize,
  speech_len: usize,
  #[serde(default)]
  speech_spans: Vec<[usize; 2]>,
}

impl Scene {
  fn has_speech(&self) -> bool {
    self.speech_len > 0
  }

  fn spans(&self) -> Vec<(usize, usize)> {
    if self.speech_spans.is_empty() {
      vec![(self.speech_start, self.speech_len)]
    } else {
      self.speech_spans.iter().map(|span| (span[0], span[1])).collect()
    }
  }
}

#[derive(Default, Clone, Copy)]
struct Tally {
  scenes: usize,
  detected: usize,
  false_alarms: usize,
  quiet_samples: usize,
}

impl Tally {
  fn detection_rate(&self) -> f64 {
    if self.scenes == 0 {
      return f64::NAN;
    }
    self.detected as f64 / self.scenes as f64
  }

  fn observed_hours(&self) -> f64 {
    self.quiet_samples as f64 / RATE / 3600.0
  }

  fn false_alarms_per_hour(&self) -> f64 {
    let hours = self.observed_hours();
    if hours <= 0.0 {
      0.0
    } else {
      self.false_alarms as f64 / hours
    }
  }
}

fn read_scene(path: &Path) -> Option<Vec<i32>> {
  let mut reader = hound::WavReader::open(path).ok()?;
  let spec = reader.spec();
  assert_eq!(spec.channels as usize, CHANNELS, "scene {path:?} is not 4-channel");
  let samples: Vec<i32> = match spec.sample_format {
    hound::SampleFormat::Float => reader
      .samples::<f32>()
      .filter_map(Result::ok)
      .map(|s| (s.clamp(-1.0, 1.0) as f64 * i32::MAX as f64) as i32)
      .collect(),
    hound::SampleFormat::Int => reader
      .samples::<i32>()
      .filter_map(Result::ok)
      .map(|s| s << (32 - spec.bits_per_sample))
      .collect(),
  };
  Some(samples)
}

fn channel_average(interleaved: &[i32]) -> Vec<f32> {
  interleaved
    .chunks_exact(CHANNELS)
    .map(|mics| (mics.iter().map(|s| *s as f64).sum::<f64>() / CHANNELS as f64 / i32::MAX as f64) as f32)
    .collect()
}

fn beamform(interleaved: &[i32], steering_deg: f64, design: Design) -> Vec<f32> {
  let mut beamformer = Beamformer::new(Config {
    steering_deg,
    design,
    ..Config::default()
  });
  let mut out = Vec::with_capacity(interleaved.len() / CHANNELS);
  beamformer.process(interleaved, &mut out);
  out
}

fn adaptive(steering_deg: f64, trusted: bool, recent_tau_s: f64, target_memory: f64) -> Beamformer {
  Beamformer::new(Config {
    steering_deg,
    design: Design::Superdirective { wng_floor_db: -6.0 },
    adaptation: Some(scene::Config {
      degrade_when_unsteered: !trusted,
      recent_tau_s,
      target_memory,
      ..scene::Config::default()
    }),
    ..Config::default()
  })
}

fn preroll(beamformer: &mut Beamformer, interleaved: &[i32], meta: &Scene, rounds: usize) {
  let lead_in = (meta.speech_start * CHANNELS).min(interleaved.len());
  let mut discard = Vec::with_capacity(lead_in / CHANNELS);
  for _ in 0..rounds {
    discard.clear();
    beamformer.process(&interleaved[..lead_in], &mut discard);
  }
  if rounds > 0 {
    beamformer.reset_frames();
  }
}

fn beamform_adaptive(
  interleaved: &[i32],
  steering_deg: f64,
  meta: &Scene,
  rounds: usize,
  trusted: bool,
  recent_tau_s: f64,
) -> Vec<f32> {
  let mut beamformer = adaptive(steering_deg, trusted, recent_tau_s, 0.0);
  preroll(&mut beamformer, interleaved, meta, rounds);
  let mut out = Vec::with_capacity(interleaved.len() / CHANNELS);
  beamformer.process(interleaved, &mut out);
  out
}

fn beamform_adaptive_rtf(
  interleaved: &[i32],
  steering_deg: f64,
  meta: &Scene,
  rounds: usize,
  recent_tau_s: f64,
  target_memory: f64,
) -> Vec<f32> {
  let mut beamformer = adaptive(steering_deg, false, recent_tau_s, target_memory);
  preroll(&mut beamformer, interleaved, meta, rounds);

  if meta.has_speech() {
    let mut cursor = 0usize;
    for (start, len) in meta.spans() {
      let end = ((start + len) * CHANNELS).min(interleaved.len());
      if end <= cursor {
        continue;
      }
      let mut discard = Vec::with_capacity((end - cursor) / CHANNELS);
      beamformer.process(&interleaved[cursor..end], &mut discard);
      beamformer.mark_target();
      cursor = end;
    }
    beamformer.reset_frames();
  }

  let mut out = Vec::with_capacity(interleaved.len() / CHANNELS);
  beamformer.process(interleaved, &mut out);
  out
}

fn run_wakeword(detector: &mut WakeWord, mono: &mut [f32], scene: &Scene) -> (bool, usize) {
  HighPass::at_array_knee(RATE).process(mono);
  detector.reset().expect("the detector should reset");

  let window = scene
    .has_speech()
    .then(|| scene.speech_start.saturating_sub(MARGIN)..scene.speech_start + scene.speech_len + MARGIN);
  let spoken: Vec<std::ops::Range<usize>> = if scene.has_speech() {
    scene
      .spans()
      .into_iter()
      .map(|(start, len)| start.saturating_sub(MARGIN)..start + len + MARGIN)
      .collect()
  } else {
    Vec::new()
  };
  let (mut detected, mut false_alarms) = (false, 0usize);
  for (index, chunk) in mono.chunks(CHUNK_SAMPLES).enumerate() {
    let Ok(Some(_)) = detector.push(chunk) else { continue };
    let at = index * CHUNK_SAMPLES;
    if window.as_ref().is_some_and(|range| range.contains(&at)) {
      detected = true;
    } else if !spoken.iter().any(|range| range.contains(&at)) {
      false_alarms += 1;
    }
  }
  (detected, false_alarms)
}

fn main() {
  let args: Vec<String> = env::args().collect();
  let (scene_dir, phrase) = (Path::new(&args[1]), Path::new(&args[2]));
  let rounds: usize = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(0);
  let tau: f64 = args
    .get(4)
    .and_then(|v| v.parse().ok())
    .unwrap_or_else(|| scene::Config::default().recent_tau_s);
  let threshold: f32 = args.get(5).and_then(|v| v.parse().ok()).unwrap_or(DEFAULT_THRESHOLD);
  let only: Vec<String> = args
    .get(6)
    .map(|csv| csv.split(',').map(|v| v.trim().to_lowercase()).collect())
    .unwrap_or_default();

  let manifest: Vec<Scene> =
    serde_json::from_slice(&std::fs::read(scene_dir.join("manifest.json")).expect("manifest.json"))
      .expect("manifest parses");

  #[allow(clippy::type_complexity)]
  let front_ends: [(&str, Box<dyn Fn(&[i32], &Scene) -> Vec<f32>>); 8] = [
    (
      "channel average",
      Box::new(|scene: &[i32], _: &Scene| channel_average(scene)),
    ),
    (
      "fixed superdirective @ broadside",
      Box::new(|scene: &[i32], _: &Scene| beamform(scene, 0.0, Design::Superdirective { wng_floor_db: -6.0 })),
    ),
    (
      "fixed superdirective @ oracle",
      Box::new(|scene: &[i32], meta: &Scene| {
        beamform(scene, meta.talker_deg, Design::Superdirective { wng_floor_db: -6.0 })
      }),
    ),
    (
      "adaptive noise @ broadside",
      Box::new(move |scene: &[i32], meta: &Scene| beamform_adaptive(scene, 0.0, meta, rounds, true, tau)),
    ),
    (
      "adaptive noise @ oracle",
      Box::new(move |scene: &[i32], meta: &Scene| beamform_adaptive(scene, meta.talker_deg, meta, rounds, true, tau)),
    ),
    (
      "adaptive noise + measured rtf",
      Box::new(move |scene: &[i32], meta: &Scene| beamform_adaptive_rtf(scene, 0.0, meta, rounds, tau, 0.0)),
    ),
    (
      "measured rtf, target memory 0.5",
      Box::new(move |scene: &[i32], meta: &Scene| beamform_adaptive_rtf(scene, 0.0, meta, rounds, tau, 0.5)),
    ),
    (
      "measured rtf, target memory 0.8",
      Box::new(move |scene: &[i32], meta: &Scene| beamform_adaptive_rtf(scene, 0.0, meta, rounds, tau, 0.8)),
    ),
  ];

  let mut detector = WakeWord::new(phrase, threshold).expect("wake word loads");
  let mut overall: BTreeMap<&str, Tally> = BTreeMap::new();
  let mut per_archetype: BTreeMap<(&str, String), Tally> = BTreeMap::new();

  for meta in &manifest {
    let Some(scene) = read_scene(&scene_dir.join(&meta.audio)) else {
      continue;
    };
    for (label, front_end) in front_ends
      .iter()
      .filter(|(label, _)| only.is_empty() || only.iter().any(|want| label.to_lowercase().contains(want)))
    {
      let mut mono = front_end(&scene, meta);
      let quiet = if meta.has_speech() {
        let spoken: usize = meta.spans().iter().map(|(_, len)| len + 2 * MARGIN).sum();
        mono.len().saturating_sub(spoken)
      } else {
        mono.len()
      };
      let (detected, false_alarms) = run_wakeword(&mut detector, &mut mono, meta);

      for tally in [
        overall.entry(label).or_default(),
        per_archetype.entry((label, meta.archetype.clone())).or_default(),
      ] {
        tally.scenes += usize::from(meta.has_speech());
        tally.detected += usize::from(detected);
        tally.false_alarms += false_alarms;
        tally.quiet_samples += quiet;
      }
    }
  }

  let median_snr = {
    let mut snrs: Vec<f64> = manifest.iter().filter_map(|s| s.snr_db).collect();
    snrs.sort_by(f64::total_cmp);
    snrs.get(snrs.len() / 2).copied().unwrap_or(f64::NAN)
  };
  let speakers: usize = manifest.iter().map(|s| s.n_speakers).sum();
  let with_speech = manifest.iter().filter(|s| s.has_speech()).count();
  println!(
    "{} scenes ({with_speech} carrying a wake word), median snr {median_snr:.1} dB, \
     {:.1} loudspeakers per scene, threshold {threshold}, {rounds} noise pre-roll(s), \
     recent tau {tau}\n",
    manifest.len(),
    speakers as f64 / manifest.len().max(1) as f64
  );

  println!(
    "{:<32} {:>9} {:>12} {:>8} {:>8}",
    "front end", "detect", "fp/hour", "alarms", "hours"
  );
  for (label, tally) in &overall {
    println!(
      "{label:<32} {:>9.3} {:>12.2} {:>8} {:>8.2}",
      tally.detection_rate(),
      tally.false_alarms_per_hour(),
      tally.false_alarms,
      tally.observed_hours()
    );
  }

  let archetypes: Vec<String> = {
    let mut seen: Vec<String> = manifest.iter().map(|s| s.archetype.clone()).collect();
    seen.sort();
    seen.dedup();
    seen
  };
  let breakdown = |title: &str, quantity: fn(&Tally) -> f64| {
    print!("\n{title:<32}");
    for archetype in &archetypes {
      print!(" {archetype:>11}");
    }
    println!();
    for (label, _) in &front_ends {
      print!("{label:<32}");
      for archetype in &archetypes {
        let value = per_archetype
          .get(&(label, archetype.clone()))
          .map(quantity)
          .unwrap_or(f64::NAN);
        print!(" {value:>11.3}");
      }
      println!();
    }
  };
  if with_speech > 0 {
    breakdown("detection by archetype", Tally::detection_rate);
  }
  if with_speech < manifest.len() {
    breakdown("fp/hour by archetype", Tally::false_alarms_per_hour);
  }
}
