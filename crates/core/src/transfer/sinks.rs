//! Receiver-side routing for inbound `TransferFragment` / `TransferAbandon`.
//! A consumer binds a sink for a transfer id before fragments can arrive
//! (pull surfaces bind at request time, push surfaces at their begin
//! request), then fragments route by id: memory sinks reassemble in place
//! under a hard cap, forward sinks relay to the owning subsystem (OTA disk
//! pump, range-proxy HTTP body) over a bounded channel. Fragments for
//! unknown ids are dropped - a cancelled or timed-out stream, not an error.

use std::{
  collections::HashMap,
  sync::{Arc, Mutex},
  time::Duration,
};

use tokio::{
  sync::{Notify, mpsc},
  time::timeout,
};
use tokio_util::bytes::{Bytes, BytesMut};
use uuid::Uuid;

pub const MEMORY_SINK_CAP: usize = 256 * 1024;
const FORWARD_CAPACITY: usize = 16;

#[derive(Debug)]
pub enum TransferEvent {
  Fragment { offset: u32, bytes: Bytes },
  Abandon { reason: String },
}

#[derive(Debug, Default)]
struct MemoryState {
  buf: BytesMut,
  failed: bool,
}

#[derive(Debug)]
enum Binding {
  Memory(MemoryState),
  Forward(mpsc::Sender<TransferEvent>),
}

#[derive(Clone, Debug, Default)]
pub struct TransferSinks {
  inner: Arc<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
  bindings: Mutex<HashMap<Uuid, Binding>>,
  progress: Notify,
}

impl TransferSinks {
  pub fn bind_memory(&self, id: Uuid) {
    self
      .inner
      .bindings
      .lock()
      .unwrap()
      .insert(id, Binding::Memory(MemoryState::default()));
  }

  pub fn bind_forward(&self, id: Uuid) -> mpsc::Receiver<TransferEvent> {
    let (tx, rx) = mpsc::channel(FORWARD_CAPACITY);
    self.inner.bindings.lock().unwrap().insert(id, Binding::Forward(tx));
    rx
  }

  pub fn unbind(&self, id: Uuid) {
    self.inner.bindings.lock().unwrap().remove(&id);
  }

  pub async fn fragment(&self, id: Uuid, offset: u32, bytes: Bytes) {
    let forward = {
      let mut bindings = self.inner.bindings.lock().unwrap();
      match bindings.get_mut(&id) {
        None => {
          tracing::trace!(%id, "fragment for unbound transfer; dropping");
          return;
        }
        Some(Binding::Memory(state)) => {
          if !state.failed {
            if offset as usize != state.buf.len() {
              tracing::warn!(%id, expected = state.buf.len(), got = offset, "transfer fragment out of order");
              state.failed = true;
            } else if state.buf.len() + bytes.len() > MEMORY_SINK_CAP {
              tracing::warn!(%id, "transfer exceeds memory sink cap");
              state.failed = true;
            } else {
              state.buf.extend_from_slice(&bytes);
            }
          }
          None
        }
        Some(Binding::Forward(tx)) => Some(tx.clone()),
      }
    };

    match forward {
      None => self.inner.progress.notify_waiters(),
      Some(tx) => {
        if tx.send(TransferEvent::Fragment { offset, bytes }).await.is_err() {
          tracing::debug!(%id, "transfer consumer gone; unbinding");
          self.unbind(id);
        }
      }
    }
  }

  pub async fn abandon(&self, id: Uuid, reason: String) {
    let forward = {
      let mut bindings = self.inner.bindings.lock().unwrap();
      match bindings.get_mut(&id) {
        None => return,
        Some(Binding::Memory(state)) => {
          tracing::debug!(%id, %reason, "transfer abandoned by sender");
          state.failed = true;
          None
        }
        Some(Binding::Forward(tx)) => Some(tx.clone()),
      }
    };

    match forward {
      None => self.inner.progress.notify_waiters(),
      Some(tx) => {
        let _ = tx.send(TransferEvent::Abandon { reason }).await;
        self.unbind(id);
      }
    }
  }

  pub async fn collect_memory(&self, id: Uuid, total_size: u32, dur: Duration) -> Option<Bytes> {
    if total_size as usize > MEMORY_SINK_CAP {
      self.unbind(id);
      return None;
    }
    let gather = async {
      loop {
        let notified = self.inner.progress.notified();
        tokio::pin!(notified);
        // register for wakeups before the check so a fragment between them is not lost
        notified.as_mut().enable();
        {
          let mut bindings = self.inner.bindings.lock().unwrap();
          match bindings.get(&id) {
            Some(Binding::Memory(state)) if state.failed => {
              bindings.remove(&id);
              return None;
            }
            Some(Binding::Memory(state)) if state.buf.len() >= total_size as usize => {
              let Some(Binding::Memory(state)) = bindings.remove(&id) else {
                return None;
              };
              return Some(state.buf.freeze());
            }
            Some(Binding::Memory(_)) => {}
            Some(Binding::Forward(_)) | None => return None,
          }
        }
        notified.await;
      }
    };
    match timeout(dur, gather).await {
      Ok(result) => result,
      Err(_) => {
        self.unbind(id);
        None
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn frag(b: &[u8]) -> Bytes {
    Bytes::copy_from_slice(b)
  }

  #[tokio::test]
  async fn memory_reassembles_in_offset_order() {
    let sinks = TransferSinks::default();
    let id = Uuid::now_v7();
    sinks.bind_memory(id);
    sinks.fragment(id, 0, frag(b"hello ")).await;
    sinks.fragment(id, 6, frag(b"world")).await;
    let got = sinks
      .collect_memory(id, 11, Duration::from_secs(1))
      .await
      .expect("collected");
    assert_eq!(&got[..], b"hello world");
  }

  #[tokio::test]
  async fn collect_resolves_when_fragments_lag() {
    let sinks = TransferSinks::default();
    let id = Uuid::now_v7();
    sinks.bind_memory(id);
    let bg = {
      let sinks = sinks.clone();
      tokio::spawn(async move { sinks.collect_memory(id, 4, Duration::from_secs(1)).await })
    };
    sinks.fragment(id, 0, frag(b"ab")).await;
    sinks.fragment(id, 2, frag(b"cd")).await;
    let got = bg.await.unwrap().expect("collected");
    assert_eq!(&got[..], b"abcd");
  }

  #[tokio::test]
  async fn out_of_order_fails_the_stream() {
    let sinks = TransferSinks::default();
    let id = Uuid::now_v7();
    sinks.bind_memory(id);
    sinks.fragment(id, 2, frag(b"late")).await;
    assert!(sinks.collect_memory(id, 4, Duration::from_millis(200)).await.is_none());
  }

  #[tokio::test]
  async fn over_cap_total_is_refused() {
    let sinks = TransferSinks::default();
    let id = Uuid::now_v7();
    sinks.bind_memory(id);
    assert!(
      sinks
        .collect_memory(id, (MEMORY_SINK_CAP + 1) as u32, Duration::from_millis(200))
        .await
        .is_none()
    );
  }

  #[tokio::test]
  async fn abandon_fails_a_pending_collect_immediately() {
    let sinks = TransferSinks::default();
    let id = Uuid::now_v7();
    sinks.bind_memory(id);
    let bg = {
      let sinks = sinks.clone();
      tokio::spawn(async move { sinks.collect_memory(id, 100, Duration::from_secs(5)).await })
    };
    sinks.fragment(id, 0, frag(b"partial")).await;
    sinks.abandon(id, "source evicted".into()).await;
    let got = tokio::time::timeout(Duration::from_millis(500), bg)
      .await
      .expect("resolves well before the collect timeout")
      .unwrap();
    assert!(got.is_none());
  }

  #[tokio::test]
  async fn forward_relays_fragments_and_abandon() {
    let sinks = TransferSinks::default();
    let id = Uuid::now_v7();
    let mut rx = sinks.bind_forward(id);
    sinks.fragment(id, 0, frag(b"aa")).await;
    sinks.abandon(id, "curl gave up".into()).await;

    match rx.recv().await.unwrap() {
      TransferEvent::Fragment { offset, bytes } => {
        assert_eq!(offset, 0);
        assert_eq!(&bytes[..], b"aa");
      }
      other => panic!("expected fragment, got {other:?}"),
    }
    match rx.recv().await.unwrap() {
      TransferEvent::Abandon { reason } => assert_eq!(reason, "curl gave up"),
      other => panic!("expected abandon, got {other:?}"),
    }
    // abandon unbinds; later fragments drop silently.
    sinks.fragment(id, 2, frag(b"bb")).await;
    assert!(rx.recv().await.is_none());
  }

  #[tokio::test]
  async fn dropped_forward_receiver_unbinds_lazily() {
    let sinks = TransferSinks::default();
    let id = Uuid::now_v7();
    let rx = sinks.bind_forward(id);
    drop(rx);
    sinks.fragment(id, 0, frag(b"aa")).await;
    assert!(sinks.inner.bindings.lock().unwrap().get(&id).is_none());
  }

  #[tokio::test]
  async fn unknown_id_fragments_drop() {
    let sinks = TransferSinks::default();
    sinks.fragment(Uuid::now_v7(), 0, frag(b"zz")).await;
  }
}
