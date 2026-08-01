#![allow(clippy::excessive_precision)]
use std::path::PathBuf;

use bridgething_wakeword::{
  WakeWord,
  features::{CHUNK_SAMPLES, EMBEDDING_DIM, Features, SAMPLE_RATE},
};
use tract_onnx::prelude::*;

fn models() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models")
}

fn phrase() -> PathBuf {
  models().join("hey_bridgething.btww")
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
  let mut features = Features::new(&models().join("embedding_stream.btww")).expect("models should load");

  let mut produced = 0;
  for chunk in audio.chunks(CHUNK_SAMPLES) {
    produced += features.push(chunk).expect("inference should succeed");
  }
  assert_eq!(produced, 48);

  let last = features.tail(1).expect("an embedding should exist");
  assert_eq!(last.len(), EMBEDDING_DIM);

  let expected = [
    -10.599433, 7.133223, 11.764214, -9.979565, 6.658922, 8.072076, 6.978046, 1.505455,
  ];
  for (i, want) in expected.iter().enumerate() {
    let got = last[i];
    assert!(
      (got - want).abs() < 0.005,
      "embedding[{i}]: expected {want:.6}, got {got:.6}"
    );
  }

  let sum: f32 = last.iter().sum();
  assert!((sum - 107.847_198).abs() < 0.05, "embedding sum was {sum:.6}");
}

#[test]
fn scores_match_the_python_reference() {
  const WINDOW: usize = 16;
  let audio = reference_audio();
  let mut features = Features::new(&models().join("embedding_stream.btww")).expect("models should load");
  for chunk in audio.chunks(CHUNK_SAMPLES) {
    features.push(chunk).expect("inference should succeed");
  }

  let classifier = tract_onnx::onnx()
    .model_for_path(models().join("hey_jarvis_v0.1.onnx"))
    .expect("the control should load")
    .into_optimized()
    .expect("the control should optimize")
    .into_runnable()
    .expect("the control should become runnable");
  let window = features.tail(WINDOW).expect("a full window should exist");
  let output = classifier
    .run(tvec!(
      Tensor::from_shape(&[1, WINDOW, EMBEDDING_DIM], window)
        .expect("window tensor")
        .into()
    ))
    .expect("control inference");
  let score = output[0].view().as_slice::<f32>().expect("f32 output")[0];

  assert!(
    (score - 0.000_168_920).abs() < 1e-6,
    "expected ~0.00016892, got {score:.9}"
  );
}

#[test]
fn a_tone_never_trips_the_detector() {
  let audio = reference_audio();
  let mut detector = WakeWord::new(&phrase(), 0.5).expect("models should load");
  for chunk in audio.chunks(CHUNK_SAMPLES) {
    assert!(detector.push(chunk).expect("inference should succeed").is_none());
  }
}

#[test]
fn chunking_of_the_input_does_not_change_the_result() {
  let audio = reference_audio();
  let score_for = |size: usize| {
    let mut detector = WakeWord::new(&phrase(), 0.5).expect("models should load");
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
  let mut detector = WakeWord::new(&phrase(), 0.5).expect("models should load");
  assert_eq!(detector.score().expect("scoring should succeed"), 0.0);
  detector
    .push(&vec![0.0; CHUNK_SAMPLES * 4])
    .expect("inference should succeed");
  assert_eq!(detector.score().expect("scoring should succeed"), 0.0);
}

#[test]
fn the_first_window_that_can_be_scored_is_reported() {
  const WINDOW: usize = 16;
  let audio = reference_audio();
  let mut detector = WakeWord::new(&phrase(), 1e-12).expect("models should load");

  let mut consumed = 0u64;
  for chunk in audio.chunks(CHUNK_SAMPLES) {
    let hit = detector.push(chunk).expect("inference should succeed");
    consumed += chunk.len() as u64;
    if detector.embedding_count() >= WINDOW {
      let hit = hit.expect("the first window with enough history should be scored, not skipped");
      assert_eq!(hit.at_sample, consumed, "reported a window other than the newest");
      return;
    }
  }
  panic!("never accumulated a full window");
}

#[test]
fn a_push_longer_than_the_feature_buffer_still_detects() {
  let audio: Vec<f32> = reference_audio()
    .iter()
    .cycle()
    .take(SAMPLE_RATE * 20)
    .copied()
    .collect();
  let first_hit = |size: usize| {
    let mut detector = WakeWord::new(&phrase(), 1e-12).expect("models should load");
    audio
      .chunks(size)
      .filter_map(|chunk| detector.push(chunk).expect("inference should succeed"))
      .next()
  };

  let chunked = first_hit(CHUNK_SAMPLES).expect("the chunked push should detect");
  let bulk = first_hit(audio.len()).expect("the bulk push should detect");
  assert_eq!(bulk.score, chunked.score, "one push detected a different first window");
}

#[test]
fn a_bulk_push_scores_every_window_a_chunked_push_does() {
  let audio = reference_audio();
  let detector = || WakeWord::new(&phrase(), 1.1).expect("models should load");

  let mut chunked = detector();
  let mut incremental = Vec::new();
  for chunk in audio.chunks(CHUNK_SAMPLES) {
    let before = chunked.embedding_count();
    chunked.push(chunk).expect("inference should succeed");
    for back in (0..chunked.embedding_count() - before).rev() {
      incremental.push(chunked.score_at(back).expect("scoring should succeed"));
    }
  }

  let mut bulk = detector();
  bulk.push(&audio).expect("inference should succeed");
  let scores: Vec<f32> = (0..incremental.len())
    .rev()
    .map(|back| bulk.score_at(back).expect("scoring should succeed"))
    .collect();

  assert_eq!(
    scores.len(),
    incremental.len(),
    "one push saw a different number of windows"
  );
  for (i, (bulk, chunked)) in scores.iter().zip(&incremental).enumerate() {
    assert!(
      (bulk - chunked).abs() < 1e-6,
      "window {i}: bulk scored {bulk:.6}, chunked scored {chunked:.6}"
    );
  }
}
