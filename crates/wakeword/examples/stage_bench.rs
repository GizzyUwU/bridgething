//! Times the three stages separately so the detector's cost is attributable.
//!
//! ```text
//! cargo run --release --example stage_bench -- <models-dir> <phrase-model> [chunks]
//! ```

use std::{env, path::PathBuf, time::Instant};

use bridgething_wakeword::{
  classifier::Classifier,
  embedding::Embedding,
  features::{CHUNK_SAMPLES, MEL_BINS, MEL_HOP, SAMPLE_RATE},
  melspec::Melspectrogram,
};
use tract_onnx::prelude::*;

const MELSPEC_INPUT_SAMPLES: usize = CHUNK_SAMPLES + MEL_HOP * 3;

type Runnable = std::sync::Arc<TypedRunnableModel>;

fn load(path: &std::path::Path, fact: Option<InferenceFact>) -> Runnable {
  let mut model = tract_onnx::onnx().model_for_path(path).expect("model should load");
  if let Some(fact) = fact {
    model = model.with_input_fact(0, fact).expect("input fact should apply");
  }
  model
    .into_optimized()
    .expect("model should optimize")
    .into_runnable()
    .expect("model should become runnable")
}

fn time<F: FnMut()>(chunks: usize, mut work: F) -> f64 {
  work();
  let start = Instant::now();
  for _ in 0..chunks {
    work();
  }
  start.elapsed().as_secs_f64() / chunks as f64
}

fn report(budget: f64, total: f64, name: &str, secs: f64) {
  println!(
    "{name:<22} {:>8.3} ms/chunk  {:>5.1}% of one core  {:>5.1}% of the stack",
    secs * 1e3,
    secs / budget * 100.0,
    secs / total * 100.0
  );
}

fn main() {
  let mut args = env::args().skip(1);
  let (Some(models), Some(phrase)) = (args.next(), args.next()) else {
    eprintln!("usage: stage_bench <models-dir> <phrase-model> [chunks]");
    std::process::exit(2);
  };
  let chunks: usize = args.next().map(|c| c.parse().expect("bad chunk count")).unwrap_or(200);
  let models = PathBuf::from(models);
  let phrase = PathBuf::from(phrase);

  let mut melspectrogram = Melspectrogram::new(SAMPLE_RATE, MEL_BINS, MEL_HOP);
  let mut embedding = Embedding::load(&models.join("embedding_stream.btww")).expect("the stack should load");
  let mut classifier = Classifier::load(&phrase).expect("the classifier should load");
  let mel_frames = embedding.frames_per_call();
  let chunks_per_call = mel_frames * MEL_HOP / CHUNK_SAMPLES;
  let batch = embedding.embeddings_per_call();
  let window = classifier.window_frames() * classifier.embedding_dim();

  let audio: Vec<f32> = (0..MELSPEC_INPUT_SAMPLES)
    .map(|i| (i as f32 * 0.031).sin() * 8000.0)
    .collect();
  let mel: Vec<f32> = (0..mel_frames * MEL_BINS)
    .map(|i| (i as f32 * 0.017).cos() + 2.0)
    .collect();
  let windows: Vec<f32> = (0..batch * window).map(|i| (i as f32 * 0.013).sin()).collect();

  let mut mel_out = Vec::new();
  let mel_s = time(chunks, || {
    melspectrogram.compute(&audio, &mut mel_out);
  });
  let embed_s = time(chunks, || {
    embedding.run(&mel).expect("embedding");
  }) / chunks_per_call as f64;
  let mut scores = Vec::new();
  let class_s = time(chunks, || {
    scores.clear();
    classifier.score(&windows, batch, &mut scores).expect("classifier");
  }) / batch as f64;

  println!("the stack takes {mel_frames} mel frames per call, {chunks_per_call} chunk(s)");
  let budget = CHUNK_SAMPLES as f64 / SAMPLE_RATE as f64;
  let total = mel_s + embed_s + class_s;
  println!(
    "chunk budget {:.1} ms ({} samples at {SAMPLE_RATE} Hz)",
    budget * 1e3,
    CHUNK_SAMPLES
  );
  for (name, secs) in [
    ("melspectrogram", mel_s),
    ("embedding", embed_s),
    ("classifier", class_s),
  ] {
    report(budget, total, name, secs);
  }
  report(budget, total, "stack total", total);

  let graphs = (models.join("embedding_stream.onnx"), phrase.with_extension("onnx"));
  if !graphs.0.exists() || !graphs.1.exists() {
    return;
  }

  println!("\nthe graphs these replaced, on tract:");
  let reference = load(&graphs.0, None);
  let caches: TVec<TValue> = (1..reference.model().inputs.len())
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
  let mel_tensor = Tensor::from_shape(&[1, mel_frames, MEL_BINS, 1], &mel).expect("mel tensor");
  let graph_embed_s = time(chunks, || {
    let mut inputs = tvec!(mel_tensor.clone().into());
    inputs.extend(caches.iter().cloned());
    reference.run(inputs).expect("embedding");
  }) / chunks_per_call as f64;

  let reference = load(
    &graphs.1,
    Some(f32::fact([batch, classifier.window_frames(), classifier.embedding_dim()]).into()),
  );
  let window_tensor = Tensor::from_shape(
    &[batch, classifier.window_frames(), classifier.embedding_dim()],
    &windows,
  )
  .expect("window tensor");
  let graph_class_s = time(chunks, || {
    reference.run(tvec!(window_tensor.clone().into())).expect("classifier");
  }) / batch as f64;

  let graph_total = mel_s + graph_embed_s + graph_class_s;
  report(budget, graph_total, "embedding on tract", graph_embed_s);
  report(budget, graph_total, "classifier on tract", graph_class_s);
  report(budget, graph_total, "stack total on tract", graph_total);
}
