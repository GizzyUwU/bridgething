use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SpeechSegment {
  pub text: String,
  pub start_ms: u64,
  pub end_ms: u64,
  pub confidence: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Transcription {
  pub text: String,
  pub alternatives: Vec<String>,
  pub segments: Vec<SpeechSegment>,
  pub confidence: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrepareEvent {
  Progress { received: u64, total: u64 },
  Ready,
  Failed { reason: String },
}

#[uniffi::export(with_foreign)]
pub trait SpeechRecognizer: Send + Sync {
  fn prepare(&self, sink: Arc<PrepareSink>);
  fn transcribe(&self, pcm: Vec<f32>, sample_rate_hz: u32, sink: Arc<TranscriptionSink>);
}

#[derive(uniffi::Object)]
pub struct PrepareSink {
  tx: mpsc::UnboundedSender<PrepareEvent>,
}

impl PrepareSink {
  pub fn channel() -> (Arc<Self>, mpsc::UnboundedReceiver<PrepareEvent>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (Arc::new(Self { tx }), rx)
  }
}

#[uniffi::export]
impl PrepareSink {
  pub fn on_progress(&self, received: u64, total: u64) {
    let _ = self.tx.send(PrepareEvent::Progress { received, total });
  }

  pub fn on_ready(&self) {
    let _ = self.tx.send(PrepareEvent::Ready);
  }

  pub fn on_failed(&self, reason: String) {
    let _ = self.tx.send(PrepareEvent::Failed { reason });
  }
}

#[derive(uniffi::Object)]
pub struct TranscriptionSink {
  tx: std::sync::Mutex<Option<oneshot::Sender<Result<Transcription, String>>>>,
}

impl TranscriptionSink {
  pub fn channel() -> (Arc<Self>, oneshot::Receiver<Result<Transcription, String>>) {
    let (tx, rx) = oneshot::channel();
    (
      Arc::new(Self {
        tx: std::sync::Mutex::new(Some(tx)),
      }),
      rx,
    )
  }

  fn settle(&self, result: Result<Transcription, String>) {
    if let Some(tx) = self.tx.lock().unwrap().take() {
      let _ = tx.send(result);
    }
  }
}

#[uniffi::export]
impl TranscriptionSink {
  pub fn complete(&self, transcription: Transcription) {
    self.settle(Ok(transcription));
  }

  pub fn fail(&self, reason: String) {
    self.settle(Err(reason));
  }
}
