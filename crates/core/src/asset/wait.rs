use std::{
  collections::HashMap,
  sync::Arc,
  time::{Duration, Instant},
};

use tokio::{
  sync::{Mutex, broadcast},
  time,
};

use super::{AssetCache, AssetCacheEvent, CachedAsset};

pub const ASSET_WAIT_TIMEOUT: Duration = Duration::from_secs(5);
pub const ASSET_STREAM_TIMEOUT: Duration = Duration::from_secs(30);
const NEGATIVE_CACHE_TTL: Duration = Duration::from_secs(5);
const INFLIGHT_BROADCAST_CAPACITY: usize = 8;

#[derive(Debug, Clone)]
pub enum FetchOutcome {
  Got(CachedAsset),
  NotFound,
}

#[derive(Debug, Clone, Default)]
pub struct AssetWaitTracker {
  inner: Arc<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
  inflight: Mutex<HashMap<String, broadcast::Sender<FetchOutcome>>>,
  negative: Mutex<HashMap<String, Instant>>,
}

impl AssetWaitTracker {
  pub fn new() -> Self {
    Self::default()
  }

  pub async fn fetch_or_wait<F, Fut>(&self, id: &str, fetch: F) -> FetchOutcome
  where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = FetchOutcome>,
  {
    if self.check_negative(id).await {
      return FetchOutcome::NotFound;
    }

    let join = {
      let mut guard = self.inner.inflight.lock().await;
      if let Some(tx) = guard.get(id) {
        Some(tx.subscribe())
      } else {
        let (tx, _) = broadcast::channel(INFLIGHT_BROADCAST_CAPACITY);
        guard.insert(id.to_string(), tx);
        None
      }
    };

    if let Some(mut rx) = join {
      return match rx.recv().await {
        Ok(outcome) => outcome,
        Err(_) => FetchOutcome::NotFound,
      };
    }

    let outcome = fetch().await;
    {
      let mut guard = self.inner.inflight.lock().await;
      if let Some(tx) = guard.remove(id) {
        let _ = tx.send(outcome.clone());
      }
    }
    if matches!(outcome, FetchOutcome::NotFound) {
      self.record_not_found(id).await;
    }
    outcome
  }

  pub async fn invalidate(&self, id: &str) {
    self.inner.negative.lock().await.remove(id);
  }

  async fn check_negative(&self, id: &str) -> bool {
    let mut guard = self.inner.negative.lock().await;
    match guard.get(id) {
      Some(at) if at.elapsed() < NEGATIVE_CACHE_TTL => true,
      Some(_) => {
        guard.remove(id);
        false
      }
      None => false,
    }
  }

  async fn record_not_found(&self, id: &str) {
    self.inner.negative.lock().await.insert(id.to_string(), Instant::now());
  }
}

pub async fn wait_for_asset(cache: &AssetCache, id: &str, timeout: Duration) -> Option<CachedAsset> {
  let mut events = cache.subscribe();
  if let Ok(Some(asset)) = cache.get(id).await {
    return Some(asset);
  }
  let deadline = Instant::now() + timeout;
  loop {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
      return None;
    }
    match time::timeout(remaining, events.recv()).await {
      Ok(Ok(AssetCacheEvent::Ready { id: ready_id })) if ready_id == id => {
        return cache.get(id).await.ok().flatten();
      }
      Ok(Ok(_)) => continue,
      Ok(Err(_)) | Err(_) => return None,
    }
  }
}

pub fn spawn_invalidator(cache: AssetCache, tracker: AssetWaitTracker) -> tokio::task::JoinHandle<()> {
  tokio::spawn(async move {
    let mut events = cache.subscribe();
    loop {
      match events.recv().await {
        Ok(AssetCacheEvent::Ready { id }) => tracker.invalidate(&id).await,
        Ok(AssetCacheEvent::Cleared { .. }) => {}
        Err(broadcast::error::RecvError::Lagged(_)) => continue,
        Err(broadcast::error::RecvError::Closed) => return,
      }
    }
  })
}

#[cfg(test)]
mod tests {
  use std::sync::atomic::{AtomicUsize, Ordering};

  use super::*;

  #[tokio::test]
  async fn coalesces_concurrent_fetches() {
    let tracker = AssetWaitTracker::new();
    let calls = Arc::new(AtomicUsize::new(0));

    let mk_fetch = |calls: Arc<AtomicUsize>| async move {
      calls.fetch_add(1, Ordering::SeqCst);
      tokio::time::sleep(Duration::from_millis(20)).await;
      FetchOutcome::NotFound
    };

    let id = "spotify/track/test/image";
    let (a, b, c) = tokio::join!(
      tracker.fetch_or_wait(id, || mk_fetch(calls.clone())),
      tracker.fetch_or_wait(id, || mk_fetch(calls.clone())),
      tracker.fetch_or_wait(id, || mk_fetch(calls.clone()))
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(matches!(a, FetchOutcome::NotFound));
    assert!(matches!(b, FetchOutcome::NotFound));
    assert!(matches!(c, FetchOutcome::NotFound));
  }

  #[tokio::test]
  async fn negative_cache_short_circuits_repeats() {
    let tracker = AssetWaitTracker::new();
    let calls = Arc::new(AtomicUsize::new(0));

    let id = "spotify/track/missing/image";
    let _ = tracker
      .fetch_or_wait(id, || {
        let calls = calls.clone();
        async move {
          calls.fetch_add(1, Ordering::SeqCst);
          FetchOutcome::NotFound
        }
      })
      .await;
    let _ = tracker
      .fetch_or_wait(id, || {
        let calls = calls.clone();
        async move {
          calls.fetch_add(1, Ordering::SeqCst);
          FetchOutcome::NotFound
        }
      })
      .await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
  }

  #[tokio::test]
  async fn invalidate_unblocks_negative_cache() {
    let tracker = AssetWaitTracker::new();
    let id = "spotify/track/late/image";
    let _ = tracker.fetch_or_wait(id, || async { FetchOutcome::NotFound }).await;
    tracker.invalidate(id).await;
    let calls = Arc::new(AtomicUsize::new(0));
    let _ = tracker
      .fetch_or_wait(id, || {
        let calls = calls.clone();
        async move {
          calls.fetch_add(1, Ordering::SeqCst);
          FetchOutcome::NotFound
        }
      })
      .await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
  }
}
