use std::path::PathBuf;

use bridgething_wakeword::{
  WakeWord,
  features::{CHUNK_SAMPLES, SAMPLE_RATE},
};

fn models() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models")
}

#[test]
fn our_own_exported_model_loads_and_scores() {
  let phrase = models().join("hey_bridgething.btww");
  if !phrase.exists() {
    eprintln!("no trained model vendored yet, skipping");
    return;
  }

  let mut detector = WakeWord::new(&phrase, 0.5).expect("our exported model should load");
  let audio: Vec<f32> = (0..SAMPLE_RATE * 3)
    .map(|n| {
      let t = n as f64 / SAMPLE_RATE as f64;
      ((0.2 * (std::f64::consts::TAU * 440.0 * t).sin()) as f32 * 32767.0) as i16 as f32 / 32767.0
    })
    .collect();

  for chunk in audio.chunks(CHUNK_SAMPLES) {
    assert!(
      detector.push(chunk).expect("inference should succeed").is_none(),
      "a pure tone must not trip the wake word"
    );
  }
  let score = detector.score().expect("scoring should succeed");
  assert!(
    (0.0..=1.0).contains(&score),
    "score {score} is outside the sigmoid range"
  );
}
