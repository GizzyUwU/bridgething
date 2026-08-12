use std::{
  path::{Path, PathBuf},
  sync::Mutex,
};

use bridgething_companion::backend::{NluModelOutputs, NluModelRunner, NluRunnerError};
use ort::{
  session::{Session, SessionOutputs, builder::GraphOptimizationLevel},
  value::Tensor,
};

use crate::backends::ModelPaths;

const MODEL_NAME: &str = "model.onnx";

struct Model {
  session: Session,
  closed_heads: usize,
}

impl Model {
  fn open(bundle_dir: &Path) -> Result<Self, String> {
    let session = Session::builder()
      .map_err(|error| error.to_string())?
      .with_optimization_level(GraphOptimizationLevel::Level3)
      .map_err(|error| error.to_string())?
      .with_intra_threads(threads())
      .map_err(|error| error.to_string())?
      .commit_from_file(bundle_dir.join(MODEL_NAME))
      .map_err(|error| error.to_string())?;

    let mut closed_heads = 0;
    while named(&session, &format!("closed_{closed_heads}")) {
      closed_heads += 1;
    }
    let claimed = session
      .outputs()
      .iter()
      .filter(|out| out.name().starts_with("closed_"))
      .count();
    if claimed != closed_heads {
      return Err(format!(
        "the model has {claimed} closed outputs but no contiguous closed_N run"
      ));
    }

    Ok(Self { session, closed_heads })
  }

  fn predict(&mut self, input_ids: &[i32], attention_mask: &[i32]) -> Result<NluModelOutputs, String> {
    let ids = int_tensor(input_ids)?;
    let mask = int_tensor(attention_mask)?;
    let out = self
      .session
      .run(ort::inputs! {
        "input_ids" => ids,
        "attention_mask" => mask,
      })
      .map_err(|error| error.to_string())?;

    Ok(NluModelOutputs {
      intent_logits: floats(&out, "intent")?,
      ood_logit: floats(&out, "ood")?.first().copied().unwrap_or_default(),
      bio_logits: floats(&out, "bio")?,
      closed_logits: (0..self.closed_heads)
        .map(|head| floats(&out, &format!("closed_{head}")))
        .collect::<Result<Vec<_>, _>>()?,
    })
  }
}

pub struct OrtNlu {
  paths: ModelPaths,
  loaded: Mutex<Option<(PathBuf, Model)>>,
}

impl OrtNlu {
  pub fn new(paths: ModelPaths) -> Self {
    Self {
      paths,
      loaded: Mutex::new(None),
    }
  }

  fn armed<'a>(&self, held: &'a mut Option<(PathBuf, Model)>) -> Result<&'a mut Model, NluRunnerError> {
    let bundle = self.paths.nlu_bundle().ok_or(NluRunnerError::NotLoaded)?;
    if held.as_ref().is_none_or(|(loaded, _)| loaded != &bundle) {
      let model = Model::open(&bundle).map_err(|reason| NluRunnerError::Failed { reason })?;
      tracing::info!(bundle = %bundle.display(), "the onnx nlu model is loaded");
      *held = Some((bundle, model));
    }
    Ok(&mut held.as_mut().expect("just armed").1)
  }
}

impl NluModelRunner for OrtNlu {
  fn prewarm(&self) {
    let mut held = self.loaded.lock().unwrap();
    if let Err(error) = self.armed(&mut held) {
      tracing::debug!(%error, "there is no onnx nlu model to prewarm");
    }
  }

  fn predict(&self, input_ids: Vec<i32>, attention_mask: Vec<i32>) -> Result<NluModelOutputs, NluRunnerError> {
    let mut held = self.loaded.lock().unwrap();
    self
      .armed(&mut held)?
      .predict(&input_ids, &attention_mask)
      .map_err(|reason| NluRunnerError::Failed { reason })
  }
}

pub fn check(bundle_dir: &Path) -> Result<(), String> {
  Model::open(bundle_dir).map(|_| ())
}

fn named(session: &Session, name: &str) -> bool {
  session.outputs().iter().any(|out| out.name() == name)
}

fn int_tensor(values: &[i32]) -> Result<Tensor<i64>, String> {
  let widened: Vec<i64> = values.iter().map(|value| i64::from(*value)).collect();
  Tensor::from_array(([1, widened.len()], widened)).map_err(|error| error.to_string())
}

fn floats(out: &SessionOutputs<'_>, name: &str) -> Result<Vec<f32>, String> {
  out
    .get(name)
    .ok_or_else(|| format!("model output {name} is missing"))?
    .try_extract_tensor::<f32>()
    .map(|(_, data)| data.to_vec())
    .map_err(|error| error.to_string())
}

fn threads() -> usize {
  std::thread::available_parallelism().map_or(1, |count| count.get().clamp(1, 4))
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use bridgething_companion::{
    api::VoiceModelPaths,
    voice::{
      inference::{BundleInference, NluInference},
      rejection::{RejectionOutcome, evaluate},
    },
  };

  use super::*;
  use crate::backends::probe::{armed, fixtures};

  fn bundle() -> PathBuf {
    fixtures().join("bundle-onnx")
  }

  #[test]
  #[ignore = "needs BRIDGETHING_VOICE_FIXTURES holding a bundle-onnx directory"]
  fn the_check_takes_a_published_desktop_bundle_and_refuses_bytes_that_are_not_one() {
    check(&bundle()).expect("the published desktop bundle opens");

    let shredded = tempfile::tempdir().expect("a scratch directory");
    std::fs::write(shredded.path().join(MODEL_NAME), b"not a model").expect("wrote the decoy");
    check(shredded.path()).expect_err("a file onnxruntime cannot parse is not a model");
  }

  #[tokio::test]
  #[ignore = "needs BRIDGETHING_VOICE_FIXTURES holding a bundle-onnx directory"]
  async fn a_published_desktop_bundle_answers_a_typed_utterance_with_an_intent() {
    let bundle = bundle();
    let runner = Arc::new(OrtNlu::new(armed(VoiceModelPaths {
      nlu_bundle_dir: Some(bundle.display().to_string()),
      asr_weights: None,
    })));

    let inference = BundleInference::load(&bundle, runner).expect("the bundle loads");
    let policy = inference.rejection().unwrap_or_default();

    for (spoken, want) in [("pause the music", "PAUSE"), ("skip this song", "NEXT")] {
      let output = inference.infer(spoken).await.expect("the model answers");
      assert_eq!(
        evaluate(&output, policy).expect("the intent head matches the catalog"),
        RejectionOutcome::Accept { intent: want },
        "onnxruntime heard {spoken:?}"
      );
    }
  }
}
