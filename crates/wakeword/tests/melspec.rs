use std::path::PathBuf;

use bridgething_wakeword::{
  features::{CHUNK_SAMPLES, MEL_BINS, MEL_HOP, SAMPLE_RATE},
  melspec::Melspectrogram,
};
use tract_onnx::prelude::*;

const INPUT_SAMPLES: usize = CHUNK_SAMPLES + MEL_HOP * 3;

fn reference() -> std::sync::Arc<TypedRunnableModel> {
  let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .join("models")
    .join("melspectrogram.onnx");
  tract_onnx::onnx()
    .model_for_path(&path)
    .expect("the reference graph should load")
    .with_input_fact(0, f32::fact([1, INPUT_SAMPLES]).into())
    .expect("input fact should apply")
    .into_optimized()
    .expect("the reference graph should optimize")
    .into_runnable()
    .expect("the reference graph should become runnable")
}

fn onnx_frames(model: &std::sync::Arc<TypedRunnableModel>, audio: &[f32]) -> Vec<f32> {
  let scaled: Vec<f32> = audio.iter().map(|s| s * 32767.0).collect();
  let input = Tensor::from_shape(&[1, INPUT_SAMPLES], &scaled).expect("input tensor");
  let output = model.run(tvec!(input.into())).expect("reference inference");
  output[0].view().as_slice::<f32>().expect("f32 output").to_vec()
}

fn signal(seed: u32, samples: usize) -> Vec<f32> {
  let mut state = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
  let mut noise = move || {
    state ^= state << 13;
    state ^= state >> 17;
    state ^= state << 5;
    (state as f32 / u32::MAX as f32) * 2.0 - 1.0
  };
  (0..samples)
    .map(|n| {
      let t = n as f64 / SAMPLE_RATE as f64;
      let envelope = (0.5 + 0.5 * (std::f64::consts::TAU * 3.0 * t).sin()) as f32;
      let voiced = (0.4 * (std::f64::consts::TAU * 140.0 * t).sin()
        + 0.3 * (std::f64::consts::TAU * 720.0 * t + 0.7).sin()
        + 0.15 * (std::f64::consts::TAU * 2300.0 * t + 1.9).sin()) as f32;
      (voiced * envelope + 0.05 * noise()) * 0.7
    })
    .collect()
}

#[test]
fn the_native_melspectrogram_matches_the_reference_graph() {
  let model = reference();
  let mut native = Melspectrogram::new(SAMPLE_RATE, MEL_BINS, MEL_HOP);
  let mut frames = Vec::new();

  let audio = signal(1, INPUT_SAMPLES + CHUNK_SAMPLES * 60);
  let mut worst = 0.0f32;
  for start in (0..audio.len() - INPUT_SAMPLES).step_by(CHUNK_SAMPLES) {
    let window = &audio[start..start + INPUT_SAMPLES];
    let expected = onnx_frames(&model, window);
    native.compute(window, &mut frames);

    assert_eq!(frames.len(), expected.len(), "frame count differs at sample {start}");
    for (got, want) in frames.iter().zip(&expected) {
      worst = worst.max((got - want).abs());
    }
  }
  assert!(
    worst < 2e-3,
    "largest disagreement with the reference graph was {worst:.3e} dB"
  );
}

#[test]
fn the_decibel_floor_is_taken_over_each_call() {
  let mut native = Melspectrogram::new(SAMPLE_RATE, MEL_BINS, MEL_HOP);
  let mut frames = Vec::new();
  native.compute(&signal(2, INPUT_SAMPLES), &mut frames);

  let top = frames.iter().copied().fold(f32::NEG_INFINITY, f32::max);
  let bottom = frames.iter().copied().fold(f32::INFINITY, f32::min);
  assert!(
    bottom >= top - 80.0 - 1e-3,
    "values ran {bottom:.3} to {top:.3}, wider than the 80 dB floor"
  );
}
