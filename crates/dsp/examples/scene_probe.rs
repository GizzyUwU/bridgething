use std::{env, path::Path};

use bridgething_dsp::{
  beamformer::Design,
  bench::{Scene, read_scene},
  geometry::CHANNELS,
  pipeline::{Beamformer, Config},
  scene,
  stft::{BINS, bin_frequencies},
};

const RATE: f64 = 16_000.0;

fn main() {
  let args: Vec<String> = env::args().collect();
  let scene_dir = Path::new(&args[1]);
  let limit: usize = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(60);
  let rounds: usize = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(0);
  let taus: Vec<f64> = args.get(4).map_or_else(
    || vec![scene::Config::default().recent_tau_s],
    |csv| csv.split(',').filter_map(|v| v.trim().parse().ok()).collect(),
  );
  let memories: Vec<f64> = args.get(5).map_or_else(
    || vec![scene::Config::default().target_memory],
    |csv| csv.split(',').filter_map(|v| v.trim().parse().ok()).collect(),
  );

  let manifest: Vec<Scene> =
    serde_json::from_slice(&std::fs::read(scene_dir.join("manifest.json")).expect("manifest.json"))
      .expect("manifest parses");

  let frequencies = bin_frequencies(RATE);
  let band: Vec<usize> = (0..BINS)
    .filter(|b| (750.0..4000.0).contains(&frequencies[*b]))
    .collect();

  let scenes: Vec<(&Scene, Vec<i32>)> = manifest
    .iter()
    .take(limit)
    .filter_map(|meta| Some((meta, read_scene(&scene_dir.join(&meta.audio))?)))
    .collect();

  println!(
    "{} scenes, {rounds} noise pre-roll(s), rtf measured over {} bins\n",
    scenes.len(),
    band.len()
  );

  let mut summaries = Vec::new();
  for (index, (tau, memory)) in taus
    .iter()
    .flat_map(|tau| memories.iter().map(move |memory| (tau, memory)))
    .enumerate()
  {
    let detailed = index == 0;
    if detailed {
      println!(
        "{:>10} {:>7} {:>7} {:>6} {:>9} {:>9} {:>8}",
        "archetype", "snr", "truth", "noise", "rtf bins", "bearing", "error"
      );
    }

    let (mut adopted_total, mut noise_total, mut errors) = (0usize, 0usize, Vec::new());
    for (meta, scene) in &scenes {
      let mut beamformer = Beamformer::new(Config {
        steering_deg: 0.0,
        design: Design::Superdirective { wng_floor_db: -6.0 },
        adaptation: Some(scene::Config {
          recent_tau_s: *tau,
          target_memory: *memory,
          ..scene::Config::default()
        }),
        ..Config::default()
      });

      let lead_in = (meta.speech_start * CHANNELS).min(scene.len());
      let mut discard = Vec::new();
      for _ in 0..rounds {
        discard.clear();
        beamformer.process(&scene[..lead_in], &mut discard);
      }
      if rounds > 0 {
        beamformer.reset_frames();
      }

      let mut cursor = 0usize;
      for (start, len) in meta.spans() {
        let end = ((start + len) * CHANNELS).min(scene.len());
        if end <= cursor {
          continue;
        }
        discard.clear();
        beamformer.process(&scene[cursor..end], &mut discard);
        beamformer.mark_target();
        cursor = end;
      }

      let (adopted, noise_measured) = beamformer.adoption();
      adopted_total += adopted;
      noise_total += usize::from(noise_measured);

      let bearing = beamformer.bearing_deg().unwrap_or(f64::NAN);
      if bearing.is_finite() {
        errors.push((bearing - meta.talker_deg).abs());
      }

      if detailed {
        println!(
          "{:>10} {:>7} {:>7.1} {:>6} {:>9} {:>9.1} {:>8.1}",
          meta.archetype,
          meta.snr_db.map(|s| format!("{s:.1}")).unwrap_or_else(|| "-".into()),
          meta.talker_deg,
          if noise_measured { "yes" } else { "no" },
          adopted,
          bearing,
          bearing - meta.talker_deg
        );
      }
    }

    errors.sort_by(f64::total_cmp);
    summaries.push((
      *tau,
      *memory,
      noise_total,
      adopted_total as f64 / scenes.len().max(1) as f64,
      errors.clone(),
    ));
  }

  println!(
    "\n{:>10} {:>7} {:>8} {:>10} {:>10} {:>10} {:>9}",
    "recent tau", "memory", "noise", "rtf bins", "resolving", "median err", "p90 err"
  );
  for (tau, memory, noise_total, adopted, errors) in &summaries {
    let (median, p90) = if errors.is_empty() {
      (f64::NAN, f64::NAN)
    } else {
      (errors[errors.len() / 2], errors[errors.len() * 9 / 10])
    };
    println!(
      "{tau:>10.2} {memory:>7.2} {:>8} {adopted:>10.1} {:>10} {median:>10.1} {p90:>9.1}",
      format!("{noise_total}/{}", scenes.len()),
      format!("{}/{}", errors.len(), scenes.len()),
    );
  }
}
