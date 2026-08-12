use std::{
  path::{Path, PathBuf},
  sync::{Arc, Mutex, Once},
};

use bridgething_companion::backend::{PrepareSink, SpeechRecognizer, SpeechSegment, Transcription, TranscriptionSink};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::backends::ModelPaths;

const SAMPLE_RATE_HZ: u32 = 16_000;
const LANGUAGE: &str = "en";
const GGML_MAGIC: [u8; 4] = [0x6c, 0x6d, 0x67, 0x67];

static LOGGING: Once = Once::new();

pub struct WhisperSpeech {
  paths: ModelPaths,
  loaded: Mutex<Option<(PathBuf, WhisperContext)>>,
}

impl WhisperSpeech {
  pub fn new(paths: ModelPaths) -> Self {
    LOGGING.call_once(whisper_rs::install_logging_hooks);
    Self {
      paths,
      loaded: Mutex::new(None),
    }
  }

  fn armed<'a>(&self, held: &'a mut Option<(PathBuf, WhisperContext)>) -> Result<&'a WhisperContext, String> {
    let weights = self
      .paths
      .asr_weights()
      .ok_or_else(|| "no asr model installed".to_owned())?;
    if held.as_ref().is_none_or(|(loaded, _)| loaded != &weights) {
      let context = WhisperContext::new_with_params(&weights, WhisperContextParameters::default())
        .map_err(|error| error.to_string())?;
      tracing::info!(weights = %weights.display(), "the whisper model is loaded");
      *held = Some((weights, context));
    }
    Ok(&held.as_ref().expect("just armed").1)
  }

  fn run(&self, pcm: &[f32]) -> Result<Transcription, String> {
    let mut held = self.loaded.lock().unwrap();
    let mut state = self.armed(&mut held)?.create_state().map_err(|e| e.to_string())?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_n_threads(threads());
    params.set_language(Some(LANGUAGE));
    params.set_detect_language(false);
    params.set_translate(false);
    params.set_no_context(true);
    params.set_temperature(0.0);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_print_special(false);
    state.full(params, pcm).map_err(|error| error.to_string())?;

    let segments: Vec<SpeechSegment> = state
      .as_iter()
      .map(|segment| SpeechSegment {
        text: segment.to_str_lossy().unwrap_or_default().trim().to_owned(),
        start_ms: segment.start_timestamp().max(0) as u64 * 10,
        end_ms: segment.end_timestamp().max(0) as u64 * 10,
        confidence: Some(mean(
          (0..segment.n_tokens()).filter_map(|index| segment.get_token(index).map(|t| t.token_probability())),
        )),
      })
      .collect();

    Ok(Transcription {
      text: segments
        .iter()
        .flat_map(|segment| segment.text.split_whitespace())
        .collect::<Vec<_>>()
        .join(" "),
      confidence: (!segments.is_empty()).then(|| mean(segments.iter().filter_map(|segment| segment.confidence))),
      alternatives: Vec::new(),
      segments,
    })
  }
}

impl SpeechRecognizer for WhisperSpeech {
  fn prepare(&self, sink: Arc<PrepareSink>) {
    let mut held = self.loaded.lock().unwrap();
    match self.armed(&mut held) {
      Ok(_) => sink.on_ready(),
      Err(reason) => sink.on_failed(reason),
    }
  }

  fn transcribe(&self, pcm: Vec<f32>, sample_rate_hz: u32, sink: Arc<TranscriptionSink>) {
    if sample_rate_hz != SAMPLE_RATE_HZ {
      sink.fail(format!(
        "whisper needs {SAMPLE_RATE_HZ} hz mono audio, got {sample_rate_hz} hz"
      ));
      return;
    }
    if pcm.is_empty() {
      sink.fail("there is nothing to transcribe".to_owned());
      return;
    }
    match self.run(&pcm) {
      Ok(transcription) => sink.complete(transcription),
      Err(reason) => sink.fail(reason),
    }
  }
}

pub fn check(weights: &Path) -> Result<(), String> {
  let mut head = [0u8; GGML_MAGIC.len()];
  std::io::Read::read_exact(
    &mut std::fs::File::open(weights).map_err(|error| error.to_string())?,
    &mut head,
  )
  .map_err(|error| error.to_string())?;
  if head != GGML_MAGIC {
    return Err("the asr model does not open with a ggml header".to_owned());
  }
  Ok(())
}

fn threads() -> std::ffi::c_int {
  std::thread::available_parallelism().map_or(1, |count| count.get().clamp(1, 4) as std::ffi::c_int)
}

fn mean(values: impl Iterator<Item = f32>) -> f32 {
  let (total, count) = values.fold((0.0, 0usize), |(total, count), value| (total + value, count + 1));
  if count == 0 { 0.0 } else { total / count as f32 }
}
