//! credit to the librespot project

use std::sync::{Arc, Once};

use tracing::{
  Event, Subscriber,
  field::{Field, Visit},
};
use tracing_subscriber::{
  EnvFilter, Layer,
  layer::{Context, SubscriberExt},
  registry::LookupSpan,
  util::SubscriberInitExt,
};

#[uniffi::export(callback_interface)]
pub trait LogSink: Send + Sync {
  fn log(&self, level: String, target: String, message: String);
}

struct SinkLayer {
  sink: Arc<dyn LogSink>,
}

#[derive(Default)]
struct MessageVisitor {
  message: String,
}

impl Visit for MessageVisitor {
  fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
    if field.name() == "message" {
      if !self.message.is_empty() {
        self.message.push(' ');
      }
      self.message.push_str(&format!("{value:?}"));
    } else {
      self.message.push_str(&format!(" {}={value:?}", field.name()));
    }
  }
}

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for SinkLayer {
  fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
    let mut visitor = MessageVisitor::default();
    event.record(&mut visitor);
    let meta = event.metadata();
    self.sink.log(
      meta.level().as_str().to_string(),
      meta.target().to_string(),
      visitor.message,
    );
  }
}

static INIT: Once = Once::new();

#[uniffi::export]
pub fn init_logging(sink: Box<dyn LogSink>, directive: String) {
  INIT.call_once(|| {
    let filter = EnvFilter::try_new(&directive).unwrap_or_else(|_| EnvFilter::new("spotify=info"));
    let sink: Arc<dyn LogSink> = Arc::from(sink);
    let layer = SinkLayer { sink: sink.clone() };
    match tracing_subscriber::registry().with(filter).with(layer).try_init() {
      Ok(()) => tracing::info!(%directive, "spotify logging initialized"),
      Err(e) => sink.log(
        "ERROR".into(),
        "spotify::logging".into(),
        format!("tracing init failed: {e}"),
      ),
    }
  });
}
