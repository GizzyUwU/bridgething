use std::{env, path::Path};

use bridgething_dsp::{
  beamformer::Design,
  bench::{Scene, read_scene},
  geometry::CHANNELS,
  pipeline::{Beamformer, Config},
  scene,
};

const CHUNK: usize = 256;

fn rms(samples: &[f32]) -> f64 {
  if samples.is_empty() {
    return 0.0;
  }
  (samples.iter().map(|s| (s * s) as f64).sum::<f64>() / samples.len() as f64).sqrt()
}

fn channel_average() -> Beamformer {
  Beamformer::new(Config {
    steering_deg: 0.0,
    design: Design::DelayAndSum,
    adaptation: None,
    ..Config::default()
  })
}

fn gains(scene: &[i32], spans: &[(usize, usize)], hold: bool) -> Vec<f64> {
  let mut beamformer = Beamformer::new(Config {
    steering_deg: 0.0,
    design: Design::Superdirective { wng_floor_db: -6.0 },
    adaptation: Some(scene::Config::default()),
    ..Config::default()
  });
  let mut reference = channel_average();

  let mark_after = spans.first().map_or(0, |(start, len)| start + len);
  let (mut beamed, mut plain) = (Vec::new(), Vec::new());
  let mut marked = false;
  for (index, chunk) in scene.chunks(CHUNK * CHANNELS).enumerate() {
    beamformer.process(chunk, &mut beamed);
    reference.process(chunk, &mut plain);
    if !marked && index * CHUNK >= mark_after {
      beamformer.mark_target();
      beamformer.hold_noise(hold);
      marked = true;
    }
  }

  spans
    .iter()
    .map(|(start, len)| {
      let (from, to) = (*start.min(&beamed.len()), (start + len).min(beamed.len()));
      let (loud, flat) = (rms(&beamed[from..to]), rms(&plain[from..to.min(plain.len())]));
      if flat <= 0.0 || loud <= 0.0 {
        f64::NAN
      } else {
        20.0 * (loud / flat).log10()
      }
    })
    .collect()
}

fn main() {
  let args: Vec<String> = env::args().collect();
  let dir = Path::new(args.get(1).expect("give a scene directory"));
  let limit: usize = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(usize::MAX);

  let manifest: Vec<Scene> =
    serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).expect("manifest.json")).expect("manifest parses");

  let mut rows: Vec<(bool, Vec<Vec<f64>>)> = vec![(false, Vec::new()), (true, Vec::new())];
  let mut by_archetype: Vec<(String, bool, f64)> = Vec::new();
  let mut scenes = 0usize;

  for meta in manifest.iter().take(limit) {
    let Some(scene) = read_scene(&dir.join(&meta.audio)) else {
      continue;
    };
    let spans = meta.spans();
    if spans.len() < 2 {
      continue;
    }
    scenes += 1;
    for (hold, collected) in &mut rows {
      let measured = gains(&scene, &spans, *hold);
      let last = measured.last().copied().unwrap_or(f64::NAN);
      by_archetype.push((meta.archetype.clone(), *hold, last));
      collected.push(measured);
    }
  }

  let longest = rows[0].1.iter().map(Vec::len).max().unwrap_or(0);
  println!("{scenes} scenes, array gain over a channel average in dB, by utterance index\n");
  print!("{:>6}", "hold");
  for index in 0..longest {
    print!(" {:>7}", format!("#{}", index + 1));
  }
  println!();
  for (hold, collected) in &rows {
    print!("{:>6}", if *hold { "yes" } else { "no" });
    for index in 0..longest {
      let values: Vec<f64> = collected
        .iter()
        .filter_map(|row| row.get(index).copied())
        .filter(|v| v.is_finite())
        .collect();
      match values.len() {
        0 => print!(" {:>7}", "-"),
        n => print!(" {:>7.2}", values.iter().sum::<f64>() / n as f64),
      }
    }
    println!();
  }

  println!("\nlast utterance, by archetype");
  let mut names: Vec<String> = by_archetype.iter().map(|(name, _, _)| name.clone()).collect();
  names.sort();
  names.dedup();
  print!("{:>6}", "hold");
  for name in &names {
    print!(" {name:>12}");
  }
  println!();
  for hold in [false, true] {
    print!("{:>6}", if hold { "yes" } else { "no" });
    for name in &names {
      let values: Vec<f64> = by_archetype
        .iter()
        .filter(|(archetype, held, value)| archetype == name && *held == hold && value.is_finite())
        .map(|(_, _, value)| *value)
        .collect();
      match values.len() {
        0 => print!(" {:>12}", "-"),
        n => print!(" {:>12.2}", values.iter().sum::<f64>() / n as f64),
      }
    }
    println!();
  }
}
