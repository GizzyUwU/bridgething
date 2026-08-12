use std::path::{Path, PathBuf};

use bridgething_wakeword::WakeWord;
use bytes::Bytes;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy)]
pub enum WakeEvent {
  Score(f32),
  Detection { score: f32, at_sample: u64 },
}

pub struct Loaded {
  pub model: PathBuf,
}

pub fn spawn(
  models: &[PathBuf],
  threshold: f32,
  events: mpsc::Sender<WakeEvent>,
) -> Option<(mpsc::Sender<Bytes>, Loaded)> {
  let (model, detector) = load(models, threshold)?;
  let (pcm_tx, pcm_rx) = mpsc::channel(16);
  std::thread::Builder::new()
    .name("wakeword".into())
    .spawn(move || run(detector, pcm_rx, events))
    .inspect_err(|err| tracing::error!("could not start the wake word thread: {err}"))
    .ok()?;
  Some((pcm_tx, Loaded { model }))
}

fn load(models: &[PathBuf], threshold: f32) -> Option<(PathBuf, WakeWord)> {
  for model in models {
    if !model.exists() {
      continue;
    }
    match WakeWord::new(model, threshold) {
      Ok(detector) => {
        tracing::info!(model = %model.display(), threshold, "wake word loaded");
        return Some((model.clone(), detector));
      }
      Err(err) => tracing::error!("wake word model {} is unusable: {err}", model.display()),
    }
  }
  tracing::warn!(
    "no wake word model at {}; the recording is still valid, scores will read zero",
    models
      .iter()
      .map(|p| p.display().to_string())
      .collect::<Vec<_>>()
      .join(", ")
  );
  None
}

fn run(mut detector: WakeWord, mut pcm: mpsc::Receiver<Bytes>, events: mpsc::Sender<WakeEvent>) {
  let mut samples: Vec<f32> = Vec::new();
  while let Some(frame) = pcm.blocking_recv() {
    samples.clear();
    bridgething_dsp::pipeline::from_pcm16(&frame, &mut samples);

    match detector.push(&samples) {
      Ok(Some(hit)) => {
        tracing::info!(score = hit.score, at_sample = hit.at_sample, "wake word fired");
        if events
          .blocking_send(WakeEvent::Detection {
            score: hit.score,
            at_sample: hit.at_sample,
          })
          .is_err()
        {
          break;
        }
      }
      Ok(None) => {}
      Err(err) => tracing::warn!("wake word inference failed: {err}"),
    }

    match detector.score() {
      Ok(score) => {
        if events.blocking_send(WakeEvent::Score(score)).is_err() {
          break;
        }
      }
      Err(err) => tracing::warn!("wake word scoring failed: {err}"),
    }
  }
  tracing::debug!("wake word thread exiting");
}

pub fn model_paths() -> Vec<PathBuf> {
  ["/var/mic-debug/wakeword", "/usr/share/mic-debug/wakeword"]
    .iter()
    .map(|dir| Path::new(dir).join("hey_bridgething.btww"))
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_missing_model_is_not_fatal() {
    let (tx, _rx) = mpsc::channel(1);
    assert!(
      spawn(&[PathBuf::from("/nowhere/model.btww")], 0.35, tx).is_none(),
      "no model means no detector, and the recorder must carry on regardless"
    );
  }

  #[test]
  fn the_search_order_prefers_the_writable_copy() {
    let paths = model_paths();
    assert!(paths[0].starts_with("/var/mic-debug"));
    assert!(paths[1].starts_with("/usr/share/mic-debug"));
  }
}
