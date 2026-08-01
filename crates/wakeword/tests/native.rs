use std::path::PathBuf;

use bridgething_wakeword::{classifier::Classifier, embedding::Embedding, features::MEL_BINS};
use tract_onnx::prelude::*;

fn models() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models")
}

struct Noise(u64);

impl Noise {
  fn next(&mut self, mean: f32, spread: f32) -> f32 {
    self.0 = self
      .0
      .wrapping_mul(6_364_136_223_846_793_005)
      .wrapping_add(1_442_695_040_888_963_407);
    let unit = ((self.0 >> 40) as f32 / (1u32 << 24) as f32) * 2.0 - 1.0;
    mean + unit * spread
  }
}

fn graph(path: &std::path::Path, fact: Option<InferenceFact>) -> std::sync::Arc<TypedRunnableModel> {
  let mut model = tract_onnx::onnx().model_for_path(path).expect("the graph should load");
  if let Some(fact) = fact {
    model = model.with_input_fact(0, fact).expect("the input fact should apply");
  }
  model
    .into_optimized()
    .expect("the graph should optimize")
    .into_runnable()
    .expect("the graph should become runnable")
}

#[test]
fn the_stack_matches_the_graph_it_replaces() {
  let mut native = Embedding::load(&models().join("embedding_stream.btww")).expect("the stack should load");
  let reference = graph(&models().join("embedding_stream.onnx"), None);

  let frames = native.frames_per_call();
  let mut caches: TVec<TValue> = (1..reference.model().inputs.len())
    .map(|input| {
      let shape = reference
        .model()
        .input_fact(input)
        .expect("cache fact")
        .shape
        .as_concrete()
        .expect("concrete cache shape")
        .to_vec();
      Tensor::zero::<f32>(&shape).expect("cache tensor").into()
    })
    .collect();

  let mut noise = Noise(7);
  let mut worst = 0.0f32;
  for _ in 0..40 {
    let mel: Vec<f32> = (0..frames * MEL_BINS).map(|_| noise.next(2.0, 2.0)).collect();

    let mut inputs = tvec!(
      Tensor::from_shape(&[1, frames, MEL_BINS, 1], &mel)
        .expect("mel tensor")
        .into()
    );
    inputs.extend(caches.iter().cloned());
    let mut outputs = reference.run(inputs).expect("reference inference");
    caches = outputs.drain(1..).collect();
    let expected = outputs[0].view().as_slice::<f32>().expect("f32 output").to_vec();

    let got = native.run(&mel).expect("native inference");
    assert_eq!(got.len(), expected.len());
    for (got, want) in got.iter().zip(&expected) {
      worst = worst.max((got - want).abs());
    }
  }

  assert!(worst < 5e-4, "worst embedding disagreement was {worst:e}");
  eprintln!("worst embedding disagreement {worst:e}");
}

#[test]
fn the_classifier_matches_the_graph_it_replaces() {
  let mut native = Classifier::load(&models().join("hey_bridgething.btww")).expect("the classifier should load");
  let (frames, dim) = (native.window_frames(), native.embedding_dim());

  let reference = graph(
    &models().join("hey_bridgething.onnx"),
    Some(f32::fact([1, frames, dim]).into()),
  );

  let mut noise = Noise(11);
  let mut worst = 0.0f32;
  for count in 1..=4 {
    let windows: Vec<f32> = (0..count * frames * dim).map(|_| noise.next(0.0, 12.0)).collect();
    let mut expected = Vec::new();
    for window in windows.chunks(frames * dim) {
      let output = reference
        .run(tvec!(
          Tensor::from_shape(&[1, frames, dim], window)
            .expect("window tensor")
            .into()
        ))
        .expect("reference inference");
      expected.extend_from_slice(output[0].view().as_slice::<f32>().expect("f32 output"));
    }

    let mut scores = Vec::new();
    native.score(&windows, count, &mut scores).expect("native scoring");
    assert_eq!(scores.len(), expected.len());
    for (got, want) in scores.iter().zip(&expected) {
      worst = worst.max((got - want).abs());
    }
  }

  assert!(worst < 1e-6, "worst score disagreement was {worst:e}");
  eprintln!("worst score disagreement {worst:e}");
}

#[test]
fn scoring_several_windows_at_once_scores_each_the_same() {
  let mut native = Classifier::load(&models().join("hey_bridgething.btww")).expect("the classifier should load");
  let (frames, dim) = (native.window_frames(), native.embedding_dim());

  let mut noise = Noise(13);
  let windows: Vec<f32> = (0..3 * frames * dim).map(|_| noise.next(0.0, 12.0)).collect();

  let mut together = Vec::new();
  native.score(&windows, 3, &mut together).expect("batched scoring");
  for (index, window) in windows.chunks_exact(frames * dim).enumerate() {
    let mut alone = Vec::new();
    native.score(window, 1, &mut alone).expect("single scoring");
    assert_eq!(
      alone[0], together[index],
      "window {index} scored differently in a batch"
    );
  }
}
