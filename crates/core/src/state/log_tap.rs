use std::{
  collections::{HashMap, VecDeque},
  future::Future,
  net::SocketAddr,
  pin::Pin,
  sync::{Arc, Mutex, RwLock},
  time::SystemTime,
};

use bluer::Address;
use libbridgething::{LogEntry, LogLevel, LogSource};
use tokio::{sync::broadcast, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{Event, Subscriber, field::Visit};
use tracing_subscriber::{Layer, layer::Context, registry::LookupSpan};
use uuid::Uuid;

const RING_CAPACITY: usize = 512;
const BROADCAST_CAPACITY: usize = 256;

const TAP_TARGET_DENYLIST: &[&str] = &[
  "bridgething::state::log_tap",
  "libbridgething::protocol",
  "bridgething::rfcomm::frame",
  "bridgething::net",
  "bridgething::ws::connection::send",
];

fn tap_denied(target: &str) -> bool {
  TAP_TARGET_DENYLIST.iter().any(|prefix| target.starts_with(prefix))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogOwner {
  Client(SocketAddr),
  Gateway(Option<Address>),
}

pub type LogSink = Box<dyn Fn(Arc<LogEntry>) -> Pin<Box<dyn Future<Output = bool> + Send>> + Send + Sync>;

#[derive(Debug, Clone)]
pub struct LogTap {
  inner: Arc<LogTapInner>,
}

#[derive(Debug)]
struct LogTapInner {
  ring: Mutex<VecDeque<LogEntry>>,
  events: broadcast::Sender<Arc<LogEntry>>,
  subscribers: RwLock<HashMap<Uuid, SubscriberEntry>>,
}

#[derive(Debug)]
struct SubscriberEntry {
  owner: LogOwner,
  cancel: CancellationToken,
  _handle: JoinHandle<()>,
}

#[derive(Debug, Clone)]
pub struct LogTapLayer {
  inner: Arc<LogTapInner>,
}

impl LogTap {
  pub fn new() -> (Self, LogTapLayer) {
    let (events, _) = broadcast::channel(BROADCAST_CAPACITY);
    let inner = Arc::new(LogTapInner {
      ring: Mutex::new(VecDeque::with_capacity(RING_CAPACITY)),
      events,
      subscribers: RwLock::new(HashMap::new()),
    });
    let layer = LogTapLayer { inner: inner.clone() };
    (Self { inner }, layer)
  }

  pub fn tail(&self, _source: LogSource, levels: &[LogLevel], filter: Option<&str>, max_lines: u32) -> Vec<LogEntry> {
    let guard = self.inner.ring.lock().expect("log tap ring poisoned");
    let mut out: Vec<LogEntry> = guard
      .iter()
      .rev()
      .filter(|entry| matches(entry, levels, filter))
      .take(max_lines as usize)
      .cloned()
      .collect();
    out.reverse();
    out
  }

  pub fn subscribe(
    &self,
    owner: LogOwner,
    sink: LogSink,
    _source: LogSource,
    levels: Vec<LogLevel>,
    filter: Option<String>,
  ) -> Uuid {
    let token = Uuid::now_v7();
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    let mut rx = self.inner.events.subscribe();
    let handle = tokio::spawn(async move {
      loop {
        tokio::select! {
          biased;
          _ = cancel_clone.cancelled() => break,
          recv = rx.recv() => match recv {
            Ok(entry) => {
              if !matches(&entry, &levels, filter.as_deref()) {
                continue;
              }
              if !sink(entry).await {
                tracing::trace!("log tap subscriber send failed; closing subscription");
                break;
              }
            }
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
              tracing::trace!(%skipped, "log tap subscriber lagged");
              continue;
            }
            Err(broadcast::error::RecvError::Closed) => break,
          }
        }
      }
    });
    self
      .inner
      .subscribers
      .write()
      .expect("log tap subscribers poisoned")
      .insert(
        token,
        SubscriberEntry {
          owner,
          cancel,
          _handle: handle,
        },
      );
    token
  }

  pub fn unsubscribe(&self, token: Uuid) -> bool {
    let removed = self
      .inner
      .subscribers
      .write()
      .expect("log tap subscribers poisoned")
      .remove(&token);
    if let Some(sub) = removed {
      sub.cancel.cancel();
      true
    } else {
      false
    }
  }

  pub fn drain_for_owner(&self, owner: LogOwner) -> Vec<Uuid> {
    let mut guard = self.inner.subscribers.write().expect("log tap subscribers poisoned");
    let tokens: Vec<Uuid> = guard
      .iter()
      .filter_map(|(token, sub)| (sub.owner == owner).then_some(*token))
      .collect();
    for token in &tokens {
      if let Some(sub) = guard.remove(token) {
        sub.cancel.cancel();
      }
    }
    tokens
  }
}

impl<S> Layer<S> for LogTapLayer
where
  S: Subscriber + for<'a> LookupSpan<'a>,
{
  fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
    let metadata = event.metadata();
    if tap_denied(metadata.target()) {
      return;
    }
    let level = match *metadata.level() {
      tracing::Level::TRACE => LogLevel::Trace,
      tracing::Level::DEBUG => LogLevel::Debug,
      tracing::Level::INFO => LogLevel::Info,
      tracing::Level::WARN => LogLevel::Warn,
      tracing::Level::ERROR => LogLevel::Error,
    };
    let mut visitor = MessageVisitor::default();
    event.record(&mut visitor);
    let entry = LogEntry {
      ts_unix_s: u32::try_from(
        SystemTime::now()
          .duration_since(SystemTime::UNIX_EPOCH)
          .map(|d| d.as_secs())
          .unwrap_or(0),
      )
      .unwrap_or(u32::MAX),
      level,
      target: metadata.target().to_string(),
      message: visitor.message,
    };

    let arc = Arc::new(entry.clone());
    {
      let mut guard = self.inner.ring.lock().expect("log tap ring poisoned");
      if guard.len() == RING_CAPACITY {
        guard.pop_front();
      }
      guard.push_back(entry);
    }
    let _ = self.inner.events.send(arc);
  }
}

#[derive(Default)]
struct MessageVisitor {
  message: String,
}

impl Visit for MessageVisitor {
  fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
    if field.name() == "message" {
      use std::fmt::Write;
      if !self.message.is_empty() {
        self.message.push(' ');
      }
      let _ = write!(self.message, "{value:?}");
    }
  }

  fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
    if field.name() == "message" {
      if !self.message.is_empty() {
        self.message.push(' ');
      }
      self.message.push_str(value);
    }
  }
}

fn matches(entry: &LogEntry, levels: &[LogLevel], filter: Option<&str>) -> bool {
  if !levels.is_empty() && !levels.contains(&entry.level) {
    return false;
  }
  if let Some(needle) = filter
    && !needle.is_empty()
    && !entry.target.contains(needle)
    && !entry.message.contains(needle)
  {
    return false;
  }
  true
}

#[cfg(test)]
mod tests {
  use super::*;

  fn entry(level: LogLevel, target: &str, message: &str) -> LogEntry {
    LogEntry {
      ts_unix_s: 0,
      level,
      target: target.into(),
      message: message.into(),
    }
  }

  #[test]
  fn matches_empty_levels_passes_all() {
    let e = entry(LogLevel::Trace, "bridgething::net", "hello");
    assert!(matches(&e, &[], None));
  }

  #[test]
  fn matches_filters_by_level() {
    let e = entry(LogLevel::Trace, "x", "y");
    assert!(!matches(&e, &[LogLevel::Info, LogLevel::Warn], None));
    assert!(matches(&e, &[LogLevel::Trace], None));
  }

  #[test]
  fn matches_substring_against_target_or_message() {
    let e = entry(LogLevel::Info, "bridgething::als", "brightness changed");
    assert!(matches(&e, &[], Some("als")));
    assert!(matches(&e, &[], Some("brightness")));
    assert!(!matches(&e, &[], Some("nope")));
  }

  #[test]
  fn tap_denied_matches_self_and_gateway_traffic_targets() {
    assert!(tap_denied("bridgething::state::log_tap"));
    assert!(tap_denied("libbridgething::protocol::bridge::encode"));
    assert!(tap_denied("libbridgething::protocol::gateway::decoder"));
    assert!(tap_denied("bridgething::rfcomm::frame"));
    assert!(tap_denied("bridgething::net::connection"));
    assert!(tap_denied("bridgething::net::connman"));
    assert!(tap_denied("bridgething::ws::connection::send"));
    assert!(!tap_denied("bridgething::bluetooth::rfcomm"));
    assert!(!tap_denied("bridgething::rfcomm::decode"));
    assert!(!tap_denied("bridgething::als"));
  }

  #[test]
  fn layer_excludes_denied_targets_from_capture() {
    use tracing_subscriber::prelude::*;
    let (tap, layer) = LogTap::new();
    let subscriber = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(subscriber, || {
      tracing::info!(target: "bridgething::als", "kept-normal");
      tracing::trace!(target: "bridgething::state::log_tap", "self-feedback");
      tracing::trace!(target: "libbridgething::protocol::bridge::encode", "codec-noise");
      tracing::trace!(target: "bridgething::rfcomm::frame", "gateway-traffic");
      tracing::trace!(target: "bridgething::ws::connection::send", "ws-send-feedback");
      tracing::trace!(target: "bridgething::net::connman", "net-send-feedback");
    });
    let messages: Vec<String> = tap
      .inner
      .ring
      .lock()
      .unwrap()
      .iter()
      .map(|e| e.message.clone())
      .collect();
    assert!(
      messages.iter().any(|m| m == "kept-normal"),
      "normal events must still be captured: {messages:?}"
    );
    assert!(
      messages.iter().all(|m| ![
        "self-feedback",
        "codec-noise",
        "gateway-traffic",
        "ws-send-feedback",
        "net-send-feedback"
      ]
      .contains(&m.as_str())),
      "denied targets must never enter the tap: {messages:?}"
    );
  }

  #[tokio::test]
  async fn tail_returns_recent_first_within_max_lines() {
    let (tap, _layer) = LogTap::new();
    {
      let mut guard = tap.inner.ring.lock().unwrap();
      for i in 0..10 {
        guard.push_back(entry(LogLevel::Info, "t", &format!("m{i}")));
      }
    }
    let out = tap.tail(LogSource::Daemon, &[], None, 3);
    let messages: Vec<&str> = out.iter().map(|e| e.message.as_str()).collect();
    assert_eq!(messages, vec!["m7", "m8", "m9"]);
  }
}
