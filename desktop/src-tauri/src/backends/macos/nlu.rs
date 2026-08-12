use std::{
  path::{Path, PathBuf},
  sync::Mutex,
};

use block2::RcBlock;
use bridgething_companion::backend::{NluModelOutputs, NluModelRunner, NluRunnerError};
use objc2::{AnyThread, rc::Retained, runtime::ProtocolObject};
use objc2_core_ml::{
  MLComputeUnits, MLDictionaryFeatureProvider, MLFeatureProvider, MLFeatureValue, MLModel, MLModelConfiguration,
  MLMultiArray, MLMultiArrayDataType,
};
use objc2_foundation::{NSArray, NSDictionary, NSError, NSNumber, NSString, NSURL};

use crate::backends::ModelPaths;

const PACKAGE_NAME: &str = "model.mlpackage";
const COMPILED_NAME: &str = "model.mlmodelc";

struct Model {
  model: Retained<MLModel>,
  closed_heads: usize,
}

// SAFETY: the retained MLModel never escapes the mutex that owns it
unsafe impl Send for Model {}

impl Model {
  fn open(bundle_dir: &Path) -> Result<Self, String> {
    let compiled = file_url(&compile(bundle_dir)?);
    let configuration = unsafe { MLModelConfiguration::new() };
    unsafe { configuration.setComputeUnits(MLComputeUnits::CPUOnly) };
    let model = unsafe { MLModel::modelWithContentsOfURL_configuration_error(&compiled, &configuration) }
      .map_err(|error| error.to_string())?;

    let outputs = unsafe { model.modelDescription().outputDescriptionsByName() };
    let mut closed_heads = 0;
    while outputs
      .objectForKey(&NSString::from_str(&format!("closed_{closed_heads}")))
      .is_some()
    {
      closed_heads += 1;
    }
    let claimed = outputs
      .keys()
      .filter(|name| name.to_string().starts_with("closed_"))
      .count();
    if claimed != closed_heads {
      return Err(format!(
        "the model has {claimed} closed outputs but no contiguous closed_N run"
      ));
    }

    Ok(Self { model, closed_heads })
  }

  fn predict(&self, input_ids: &[i32], attention_mask: &[i32]) -> Result<NluModelOutputs, String> {
    let ids = int_array(input_ids)?;
    let mask = int_array(attention_mask)?;
    let features = unsafe {
      MLDictionaryFeatureProvider::initWithDictionary_error(
        MLDictionaryFeatureProvider::alloc(),
        NSDictionary::from_slices(
          &[
            &*NSString::from_str("input_ids"),
            &*NSString::from_str("attention_mask"),
          ],
          &[
            MLFeatureValue::featureValueWithMultiArray(&ids).as_ref(),
            MLFeatureValue::featureValueWithMultiArray(&mask).as_ref(),
          ],
        )
        .as_ref(),
      )
    }
    .map_err(|error| error.to_string())?;

    let out = unsafe {
      self
        .model
        .predictionFromFeatures_error(ProtocolObject::from_ref(&*features))
    }
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

pub struct CoreMlNlu {
  paths: ModelPaths,
  loaded: Mutex<Option<(PathBuf, Model)>>,
}

impl CoreMlNlu {
  pub fn new(paths: ModelPaths) -> Self {
    Self {
      paths,
      loaded: Mutex::new(None),
    }
  }

  fn armed<'a>(&self, held: &'a mut Option<(PathBuf, Model)>) -> Result<&'a Model, NluRunnerError> {
    let bundle = self.paths.nlu_bundle().ok_or(NluRunnerError::NotLoaded)?;
    if held.as_ref().is_none_or(|(loaded, _)| loaded != &bundle) {
      let model = Model::open(&bundle).map_err(|reason| NluRunnerError::Failed { reason })?;
      tracing::info!(bundle = %bundle.display(), "the coreml nlu model is loaded");
      *held = Some((bundle, model));
    }
    Ok(&held.as_ref().expect("just armed").1)
  }
}

impl NluModelRunner for CoreMlNlu {
  fn prewarm(&self) {
    let mut held = self.loaded.lock().unwrap();
    if let Err(error) = self.armed(&mut held) {
      tracing::debug!(%error, "there is no coreml nlu model to prewarm");
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

fn compile(bundle_dir: &Path) -> Result<PathBuf, String> {
  let cached = bundle_dir.join(COMPILED_NAME);
  if cached.exists() {
    return Ok(cached);
  }

  let staged = PathBuf::from(compile_package(&file_url(&bundle_dir.join(PACKAGE_NAME)))?);
  match std::fs::rename(&staged, &cached) {
    Ok(()) => Ok(cached),
    Err(error) => {
      tracing::warn!(%error, "the compiled model stays in its temporary home and is compiled again next launch");
      Ok(staged)
    }
  }
}

fn compile_package(package: &NSURL) -> Result<String, String> {
  let (tx, rx) = std::sync::mpsc::channel();
  let handler = RcBlock::new(move |compiled: *mut NSURL, error: *mut NSError| {
    let _ = tx.send(
      unsafe { compiled.as_ref() }
        .and_then(NSURL::path)
        .map(|path| path.to_string())
        .ok_or_else(|| {
          unsafe { error.as_ref() }.map_or_else(|| "coreml refused the model".to_owned(), NSError::to_string)
        }),
    );
  });
  unsafe { MLModel::compileModelAtURL_completionHandler(package, &handler) };
  rx.recv()
    .map_err(|_| "coreml never answered the compile request".to_owned())?
}

pub fn check(bundle_dir: &Path) -> Result<(), String> {
  Model::open(bundle_dir).map(|_| ())
}

fn file_url(path: &Path) -> Retained<NSURL> {
  NSURL::fileURLWithPath_isDirectory(&NSString::from_str(&path.to_string_lossy()), true)
}

fn int_array(values: &[i32]) -> Result<Retained<MLMultiArray>, String> {
  let shape = NSArray::from_retained_slice(&[NSNumber::new_isize(1), NSNumber::new_isize(values.len() as isize)]);
  let array =
    unsafe { MLMultiArray::initWithShape_dataType_error(MLMultiArray::alloc(), &shape, MLMultiArrayDataType::Int32) }
      .map_err(|error| error.to_string())?;
  for (index, value) in values.iter().enumerate() {
    unsafe { array.setObject_atIndexedSubscript(&NSNumber::new_i32(*value), index as isize) };
  }
  Ok(array)
}

fn floats(out: &ProtocolObject<dyn MLFeatureProvider>, name: &str) -> Result<Vec<f32>, String> {
  let array = unsafe { out.featureValueForName(&NSString::from_str(name)) }
    .and_then(|value| unsafe { value.multiArrayValue() })
    .ok_or_else(|| format!("model output {name} is missing"))?;
  let count = unsafe { array.count() }.max(0) as usize;
  Ok(
    (0..count)
      .map(|index| unsafe { array.objectAtIndexedSubscript(index as isize) }.as_f32())
      .collect(),
  )
}
