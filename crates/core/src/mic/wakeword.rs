use std::path::PathBuf;

use bridgething_wakeword::WakeWord;
use bytes::Bytes;
use tokio::sync::mpsc;

use super::Detection;

const BACKLOG: usize = 8;

#[derive(Debug)]
pub struct WakeWordLink {
  pcm: mpsc::Sender<Bytes>,
}

impl WakeWordLink {
  pub fn spawn(models: &[PathBuf], threshold: f32) -> Option<(Self, mpsc::Receiver<Detection>)> {
    let detector = load(models, threshold)?;
    let (pcm_tx, pcm_rx) = mpsc::channel(BACKLOG);
    let (hit_tx, hit_rx) = mpsc::channel(4);
    std::thread::Builder::new()
      .name("wakeword".into())
      .spawn(move || run(detector, pcm_rx, hit_tx))
      .inspect_err(|err| tracing::error!("could not start the wake word thread: {err}"))
      .ok()?;
    Some((Self { pcm: pcm_tx }, hit_rx))
  }

  pub fn offer(&self, pcm: Bytes) {
    if self.pcm.try_send(pcm).is_err() {
      tracing::trace!("wake word is behind; dropping a frame");
    }
  }
}

fn load(models: &[PathBuf], threshold: f32) -> Option<WakeWord> {
  for model in models {
    if !model.exists() {
      continue;
    }
    match WakeWord::new(model, threshold) {
      Ok(detector) => {
        tracing::info!(model = %model.display(), threshold, "wake word loaded");
        return Some(detector);
      }
      Err(err) => tracing::error!("wake word model {} is unusable: {err}", model.display()),
    }
  }
  tracing::warn!(
    "no wake word model at {}; voice needs push to talk",
    models
      .iter()
      .map(|p| p.display().to_string())
      .collect::<Vec<_>>()
      .join(", ")
  );
  None
}

fn run(mut detector: WakeWord, mut pcm: mpsc::Receiver<Bytes>, hits: mpsc::Sender<Detection>) {
  let mut samples: Vec<f32> = Vec::new();
  while let Some(frame) = pcm.blocking_recv() {
    samples.clear();
    bridgething_dsp::pipeline::from_pcm16(&frame, &mut samples);
    match detector.push(&samples) {
      Ok(Some(hit)) => {
        tracing::info!(score = hit.score, at_sample = hit.at_sample, "wake word fired");
        if hits.blocking_send(Detection { score: hit.score }).is_err() {
          break;
        }
      }
      Ok(None) => {}
      Err(err) => tracing::warn!("wake word inference failed: {err}"),
    }
  }
  tracing::debug!("wake word thread exiting");
}
