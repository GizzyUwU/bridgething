//! Scores the voice endpointer on rendered scenes, through the front end the daemon actually runs.
//!
//! ```text
//! cargo run --release --example vad_bench -- turns=<dir> pause=<dir> silent=<dir> [...]
//! ```

use std::{
  collections::BTreeMap,
  env,
  path::Path,
  time::{Duration, Instant},
};

use bridgething_dsp::{
  geometry::CHANNELS,
  pipeline::{Beamformer, Config, to_pcm16},
  scene,
  vad::{Config as VadConfig, FRAME_SAMPLES, TurnEnd, VoiceEndpointer},
};
use serde::Deserialize;

const RATE: f64 = 16_000.0;
const LEAD: usize = 8_000;
const PATIENCE: usize = 48_000;

const HANGOVERS_MS: [u64; 3] = [500, 800, 1200];
const THRESHOLDS: [f32; 5] = [0.30, 0.40, 0.50, 0.60, 0.70];

#[derive(Deserialize)]
struct Scene {
  audio: String,
  archetype: String,
  speech_start: usize,
  speech_len: usize,
  #[serde(default)]
  speech_spans: Vec<[usize; 2]>,
}

impl Scene {
  fn spans(&self) -> Vec<(usize, usize)> {
    if self.speech_spans.is_empty() {
      if self.speech_len == 0 {
        Vec::new()
      } else {
        vec![(self.speech_start, self.speech_len)]
      }
    } else {
      self.speech_spans.iter().map(|span| (span[0], span[1])).collect()
    }
  }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
  Turns,
  Pause,
  Silent,
}

struct Turn {
  archetype: String,
  arm_at: usize,
  speech_end: usize,
  limit: usize,
  pause: Option<usize>,
}

#[derive(Default)]
struct Outcome {
  latencies_ms: Vec<f64>,
  cuts: usize,
  onset_missed: usize,
  never_closed: usize,
  onset_fired: usize,
  turns: usize,
}

impl Outcome {
  fn percentile(&self, fraction: f64) -> f64 {
    if self.latencies_ms.is_empty() {
      return f64::NAN;
    }
    let mut sorted = self.latencies_ms.clone();
    sorted.sort_by(f64::total_cmp);
    let index = ((sorted.len() - 1) as f64 * fraction).round() as usize;
    sorted[index]
  }

  fn rate(&self, count: usize) -> f64 {
    if self.turns == 0 {
      f64::NAN
    } else {
      count as f64 / self.turns as f64
    }
  }

  fn absorb(&mut self, other: &Outcome) {
    self.latencies_ms.extend_from_slice(&other.latencies_ms);
    self.cuts += other.cuts;
    self.onset_missed += other.onset_missed;
    self.never_closed += other.never_closed;
    self.onset_fired += other.onset_fired;
    self.turns += other.turns;
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

fn front_end(interleaved: &[i32], spans: &[(usize, usize)]) -> Vec<f32> {
  let mut beamformer = Beamformer::new(Config {
    adaptation: Some(scene::Config::default()),
    ..Config::default()
  });

  let mut cursor = 0usize;
  for (start, len) in spans {
    let end = ((start + len) * CHANNELS).min(interleaved.len());
    if end <= cursor {
      continue;
    }
    let mut discard = Vec::with_capacity((end - cursor) / CHANNELS);
    beamformer.process(&interleaved[cursor..end], &mut discard);
    beamformer.mark_target();
    cursor = end;
  }
  if !spans.is_empty() {
    beamformer.reset_frames();
  }

  let mut out = Vec::with_capacity(interleaved.len() / CHANNELS);
  beamformer.process(interleaved, &mut out);

  let mut encoded = Vec::with_capacity(out.len() * 2);
  to_pcm16(&out, &mut encoded);
  encoded
    .chunks_exact(2)
    .map(|pair| i16::from_le_bytes([pair[0], pair[1]]) as f32 / 32768.0)
    .collect()
}

fn turns_of(meta: &Scene, kind: Kind, total: usize) -> Vec<Turn> {
  let spans = meta.spans();
  let turn = |arm_at: usize, speech_end: usize, limit: usize, pause: Option<usize>| Turn {
    archetype: meta.archetype.clone(),
    arm_at,
    speech_end,
    limit: limit.min(total),
    pause,
  };
  match kind {
    Kind::Silent => vec![turn(meta.speech_start.saturating_sub(LEAD), 0, total, None)],
    Kind::Turns => spans
      .iter()
      .enumerate()
      .map(|(index, (start, len))| {
        let floor = index.checked_sub(1).map_or(0, |prev| spans[prev].0 + spans[prev].1);
        let next = spans.get(index + 1).map_or(usize::MAX, |(start, _)| *start);
        turn(
          start.saturating_sub(LEAD).max(floor),
          start + len,
          (start + len + PATIENCE).min(next),
          None,
        )
      })
      .collect(),
    Kind::Pause => {
      let (Some(first), Some(last)) = (spans.first(), spans.last()) else {
        return Vec::new();
      };
      if spans.len() < 2 || last.0 <= first.0 + first.1 {
        return Vec::new();
      }
      vec![turn(
        first.0.saturating_sub(LEAD),
        last.0 + last.1,
        last.0 + last.1 + PATIENCE,
        Some(last.0 - (first.0 + first.1)),
      )]
    }
  }
}

struct Cost {
  elapsed: Duration,
  frames: usize,
}

fn run_turn(mono: &[f32], turn: &Turn, config: VadConfig, cost: &mut Cost) -> Outcome {
  let mut endpointer = VoiceEndpointer::new(config);
  let mut outcome = Outcome {
    turns: 1,
    ..Outcome::default()
  };

  let window = &mono[turn.arm_at.min(mono.len())..turn.limit.min(mono.len())];
  let started = Instant::now();
  let mut closed = None;
  for (index, frame) in window.chunks_exact(FRAME_SAMPLES).enumerate() {
    cost.frames += 1;
    if let Some(end) = endpointer.observe(frame) {
      closed = Some((turn.arm_at + (index + 1) * FRAME_SAMPLES, end));
      break;
    }
  }
  cost.elapsed += started.elapsed();

  outcome.onset_fired = usize::from(endpointer.speech_seen());
  match closed {
    None => outcome.never_closed = 1,
    Some((_, TurnEnd::NoOnset)) => outcome.onset_missed = 1,
    Some((at, TurnEnd::SilenceAfterSpeech)) => {
      if at < turn.speech_end {
        outcome.cuts = 1;
      } else {
        outcome.latencies_ms.push((at - turn.speech_end) as f64 / RATE * 1000.0);
      }
    }
  }
  outcome
}

fn pause_bucket(samples: usize) -> &'static str {
  match (samples as f64 / RATE * 1000.0) as u64 {
    0..=399 => "<0.4s",
    400..=699 => "0.4-0.7s",
    700..=999 => "0.7-1.0s",
    1000..=1399 => "1.0-1.4s",
    _ => ">1.4s",
  }
}

fn main() {
  let mut sets: Vec<(Kind, String)> = Vec::new();
  for arg in env::args().skip(1) {
    let (kind, dir) = arg.split_once('=').expect("arguments are kind=dir");
    let kind = match kind {
      "turns" => Kind::Turns,
      "pause" => Kind::Pause,
      "silent" => Kind::Silent,
      other => panic!("unknown scene kind {other}"),
    };
    sets.push((kind, dir.to_string()));
  }
  assert!(!sets.is_empty(), "give at least one kind=dir");

  let grid: Vec<(u64, f32)> = HANGOVERS_MS
    .iter()
    .flat_map(|hangover| THRESHOLDS.iter().map(move |threshold| (*hangover, *threshold)))
    .collect();

  let mut by_setting: BTreeMap<(Kind, u64, u32), Outcome> = BTreeMap::new();
  let mut by_archetype: BTreeMap<(Kind, u64, u32, String), Outcome> = BTreeMap::new();
  let mut by_pause: BTreeMap<(u64, u32, &'static str), Outcome> = BTreeMap::new();
  let mut cost = Cost {
    elapsed: Duration::ZERO,
    frames: 0,
  };
  let mut scenes_read = 0usize;

  for (kind, dir) in &sets {
    let dir = Path::new(dir);
    let manifest: Vec<Scene> =
      serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).expect("manifest.json"))
        .expect("manifest parses");
    eprintln!("{} scenes from {}", manifest.len(), dir.display());

    for meta in &manifest {
      let Some(interleaved) = read_scene(&dir.join(&meta.audio)) else {
        continue;
      };
      scenes_read += 1;
      let mono = front_end(&interleaved, &meta.spans());
      let turns = turns_of(meta, *kind, mono.len());

      for turn in &turns {
        for (hangover, threshold) in &grid {
          let config = VadConfig {
            threshold: *threshold,
            hangover: Duration::from_millis(*hangover),
            ..VadConfig::default()
          };
          let outcome = run_turn(&mono, turn, config, &mut cost);
          let key = (*kind, *hangover, threshold.to_bits());
          by_setting.entry(key).or_default().absorb(&outcome);
          by_archetype
            .entry((key.0, key.1, key.2, turn.archetype.clone()))
            .or_default()
            .absorb(&outcome);
          if let Some(pause) = turn.pause {
            by_pause
              .entry((*hangover, threshold.to_bits(), pause_bucket(pause)))
              .or_default()
              .absorb(&outcome);
          }
        }
      }
    }
  }

  println!(
    "\n{scenes_read} scenes, onset window {:?}, min speech {:?}",
    VadConfig::default().onset,
    VadConfig::default().min_speech
  );
  println!(
    "endpointer cost: {:.1} us per 16 ms frame over {} frames ({:.4}x realtime on this host)",
    cost.elapsed.as_secs_f64() * 1e6 / cost.frames.max(1) as f64,
    cost.frames,
    cost.elapsed.as_secs_f64() * 1e6 / cost.frames.max(1) as f64 / 16_000.0
  );

  if by_setting.keys().any(|k| k.0 == Kind::Turns) {
    println!("\nindependent turns: close latency past the end of the utterance");
    println!(
      "{:>9} {:>10} {:>9} {:>9} {:>10} {:>11} {:>12} {:>7}",
      "hangover", "threshold", "p50 ms", "p90 ms", "false cut", "onset miss", "never closed", "turns"
    );
    for ((kind, hangover, threshold), outcome) in &by_setting {
      if *kind != Kind::Turns {
        continue;
      }
      println!(
        "{hangover:>9} {:>10.2} {:>9.0} {:>9.0} {:>10.4} {:>11.4} {:>12.4} {:>7}",
        f32::from_bits(*threshold),
        outcome.percentile(0.5),
        outcome.percentile(0.9),
        outcome.rate(outcome.cuts),
        outcome.rate(outcome.onset_missed),
        outcome.rate(outcome.never_closed),
        outcome.turns
      );
    }
  }

  if !by_pause.is_empty() {
    println!("\nutterance with a pause in it: rate of closing before the talker is done");
    let buckets = ["<0.4s", "0.4-0.7s", "0.7-1.0s", "1.0-1.4s", ">1.4s"];
    print!("{:>9} {:>10}", "hangover", "threshold");
    for bucket in buckets {
      print!(" {bucket:>10}");
    }
    println!("{:>8}", "p50 ms");
    for (hangover, threshold) in &grid {
      print!("{hangover:>9} {:>10.2}", threshold);
      let mut whole = Outcome::default();
      for bucket in buckets {
        match by_pause.get(&(*hangover, threshold.to_bits(), bucket)) {
          Some(outcome) => {
            print!(" {:>10.3}", outcome.rate(outcome.cuts));
            whole.absorb(outcome);
          }
          None => print!(" {:>10}", "-"),
        }
      }
      println!("{:>8.0}", whole.percentile(0.5));
    }
    print!("{:>20}", "turns per bucket");
    for bucket in buckets {
      let turns = by_pause
        .get(&(grid[0].0, grid[0].1.to_bits(), bucket))
        .map_or(0, |o| o.turns);
      print!(" {turns:>10}");
    }
    println!();
  }

  if by_setting.keys().any(|k| k.0 == Kind::Silent) {
    println!("\ntalker muted: the interferers alone must not read as a talker");
    println!(
      "{:>10} {:>12} {:>13} {:>7}",
      "threshold", "onset fired", "closed silent", "turns"
    );
    for threshold in THRESHOLDS {
      let Some(outcome) = by_setting.get(&(Kind::Silent, HANGOVERS_MS[0], threshold.to_bits())) else {
        continue;
      };
      println!(
        "{threshold:>10.2} {:>12.4} {:>13.4} {:>7}",
        outcome.rate(outcome.onset_fired),
        outcome.rate(outcome.onset_missed),
        outcome.turns
      );
    }
  }

  let archetypes: Vec<String> = {
    let mut seen: Vec<String> = by_archetype.keys().map(|(_, _, _, a)| a.clone()).collect();
    seen.sort();
    seen.dedup();
    seen
  };
  if !archetypes.is_empty() && by_archetype.keys().any(|k| k.0 == Kind::Turns) {
    println!("\nindependent turns by archetype: p50 / p90 close latency in ms");
    print!("{:>9} {:>10}", "hangover", "threshold");
    for archetype in &archetypes {
      print!(" {archetype:>16}");
    }
    println!();
    for (hangover, threshold) in &grid {
      print!("{hangover:>9} {:>10.2}", threshold);
      for archetype in &archetypes {
        match by_archetype.get(&(Kind::Turns, *hangover, threshold.to_bits(), archetype.clone())) {
          Some(outcome) => print!(" {:>7.0} /{:>7.0}", outcome.percentile(0.5), outcome.percentile(0.9)),
          None => print!(" {:>16}", "-"),
        }
      }
      println!();
    }
  }
}
