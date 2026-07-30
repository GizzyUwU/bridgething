#![allow(clippy::excessive_precision)]
use std::path::PathBuf;

use bridgething_wakeword::{
  WakeWord,
  features::{CHUNK_SAMPLES, EMBEDDING_DIM, Features, SAMPLE_RATE},
};

fn models() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models")
}

fn reference_audio() -> Vec<f32> {
  (0..SAMPLE_RATE * 4)
    .map(|n| {
      let t = n as f64 / SAMPLE_RATE as f64;
      let sum = 0.3 * (std::f64::consts::TAU * 440.0 * t).sin()
        + 0.2 * (std::f64::consts::TAU * 1000.0 * t + 1.0).sin()
        + 0.1 * (std::f64::consts::TAU * 2500.0 * t + 2.0).sin();
      ((sum as f32) * 32767.0) as i16 as f32 / 32767.0
    })
    .collect()
}

#[test]
fn embeddings_match_the_python_reference() {
  let audio = reference_audio();
  let mut features = Features::new(
    &models().join("melspectrogram.onnx"),
    &models().join("embedding_model.onnx"),
  )
  .expect("models should load");

  let mut produced = 0;
  for chunk in audio.chunks(CHUNK_SAMPLES) {
    produced += features.push(chunk).expect("inference should succeed");
  }
  assert_eq!(produced, 49);

  let last = features.tail(1).expect("an embedding should exist");
  assert_eq!(last.len(), EMBEDDING_DIM);

  let expected = [
    -10.645019, 7.108760, 11.795203, -9.942851, 6.655813, 8.063003, 6.951853, 1.491260,
  ];
  for (i, want) in expected.iter().enumerate() {
    let got = last[i];
    assert!(
      (got - want).abs() < 0.05,
      "embedding[{i}]: expected {want:.6}, got {got:.6}"
    );
  }

  let sum: f32 = last.iter().sum();
  assert!((sum - 107.886_292).abs() < 0.5, "embedding sum was {sum:.6}");
}

#[test]
fn scores_match_the_python_reference() {
  let audio = reference_audio();
  let mut detector = WakeWord::new(&models(), &models().join("hey_jarvis_v0.1.onnx"), 0.5).expect("models should load");

  for chunk in audio.chunks(CHUNK_SAMPLES) {
    detector.push(chunk).expect("inference should succeed");
  }
  let score = detector.score().expect("scoring should succeed");
  assert!((score - 0.000169).abs() < 1e-4, "expected ~0.000169, got {score:.6}");
}

#[test]
fn a_tone_never_trips_the_detector() {
  let audio = reference_audio();
  let mut detector = WakeWord::new(&models(), &models().join("hey_jarvis_v0.1.onnx"), 0.5).expect("models should load");
  for chunk in audio.chunks(CHUNK_SAMPLES) {
    assert!(detector.push(chunk).expect("inference should succeed").is_none());
  }
}

#[test]
fn chunking_of_the_input_does_not_change_the_result() {
  let audio = reference_audio();
  let score_for = |size: usize| {
    let mut detector =
      WakeWord::new(&models(), &models().join("hey_jarvis_v0.1.onnx"), 0.5).expect("models should load");
    for chunk in audio.chunks(size) {
      detector.push(chunk).expect("inference should succeed");
    }
    detector.score().expect("scoring should succeed")
  };
  let reference = score_for(CHUNK_SAMPLES);
  for size in [128, 256, 320, 1000] {
    let got = score_for(size);
    assert!(
      (got - reference).abs() < 1e-6,
      "chunk size {size} gave {got:.6} vs {reference:.6}"
    );
  }
}

#[test]
fn score_is_zero_until_enough_history_exists() {
  let mut detector = WakeWord::new(&models(), &models().join("hey_jarvis_v0.1.onnx"), 0.5).expect("models should load");
  assert_eq!(detector.score().expect("scoring should succeed"), 0.0);
  detector
    .push(&vec![0.0; CHUNK_SAMPLES * 4])
    .expect("inference should succeed");
  assert_eq!(detector.score().expect("scoring should succeed"), 0.0);
}
