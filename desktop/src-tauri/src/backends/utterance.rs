use std::{
  sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
  },
  time::Duration,
};

use bridgething_companion::backend::SpeakSink;

const SECONDS_PER_CHAR: f64 = 0.12;
const SPEECH_FLOOR: f64 = 15.0;
const SPEECH_SLACK: f64 = 10.0;

struct Speaking {
  token: u64,
  sink: Arc<SpeakSink>,
}

#[derive(Default)]
pub struct Utterances {
  current: Mutex<Option<Speaking>>,
  next: AtomicU64,
}

impl Utterances {
  pub fn begin(self: &Arc<Self>, sink: Arc<SpeakSink>, text: &str) {
    let token = self.next.fetch_add(1, Ordering::Relaxed);
    let displaced = self.current.lock().unwrap().replace(Speaking { token, sink });
    if let Some(previous) = displaced {
      previous.sink.on_finished(false);
    }
    self.watchdog(token, text);
  }

  pub fn started(&self) {
    let held = self
      .current
      .lock()
      .unwrap()
      .as_ref()
      .map(|speaking| speaking.sink.clone());
    if let Some(sink) = held {
      sink.on_start();
    }
  }

  pub fn finish(&self, completed: bool) {
    if let Some(speaking) = self.current.lock().unwrap().take() {
      speaking.sink.on_finished(completed);
    }
  }

  fn watchdog(self: &Arc<Self>, token: u64, text: &str) {
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
      return;
    };
    let deadline =
      Duration::from_secs_f64((text.chars().count() as f64 * SECONDS_PER_CHAR + SPEECH_SLACK).max(SPEECH_FLOOR));
    let held = Arc::clone(self);
    handle.spawn(async move {
      tokio::time::sleep(deadline).await;
      held.expire(token);
    });
  }

  fn expire(&self, token: u64) {
    let stalled = {
      let mut held = self.current.lock().unwrap();
      match held.as_ref() {
        Some(speaking) if speaking.token == token => held.take(),
        _ => None,
      }
    };
    if let Some(speaking) = stalled {
      tracing::warn!(
        token,
        "speech synthesis never reported an end; the utterance is abandoned"
      );
      speaking.sink.on_finished(false);
    }
  }
}
