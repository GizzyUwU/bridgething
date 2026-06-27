use std::future::Future;

use libbridgething::BrowseResult;
use tokio::{
  sync::Mutex,
  time::{Duration, Instant},
};

#[derive(Debug)]
struct Cached {
  generation: u64,
  fetched_at: Instant,
  result: BrowseResult,
}

#[derive(Debug, Default)]
pub struct RootBrowseCache {
  inner: Mutex<Option<Cached>>,
}

impl RootBrowseCache {
  pub async fn get_or_fetch<F, Fut, E>(&self, generation: u64, ttl: Duration, fetch: F) -> Result<BrowseResult, E>
  where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<BrowseResult, E>>,
  {
    let mut guard = self.inner.lock().await;
    if let Some(cached) = guard.as_ref()
      && cached.generation == generation
      && cached.fetched_at.elapsed() < ttl
    {
      return Ok(cached.result.clone());
    }
    let result = fetch().await?;
    *guard = Some(Cached {
      generation,
      fetched_at: Instant::now(),
      result: result.clone(),
    });
    Ok(result)
  }
}

#[cfg(test)]
mod tests {
  use std::sync::atomic::{AtomicU32, Ordering};

  use super::*;

  fn result() -> BrowseResult {
    BrowseResult {
      entries: Vec::new(),
      total: Some(7),
      has_more: false,
    }
  }

  const LONG_TTL: Duration = Duration::from_secs(3600);

  #[tokio::test]
  async fn same_generation_within_ttl_reuses_cache() {
    let cache = RootBrowseCache::default();
    let calls = AtomicU32::new(0);
    let fetch = || {
      cache.get_or_fetch(0, LONG_TTL, || async {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok::<_, ()>(result())
      })
    };
    fetch().await.unwrap();
    fetch().await.unwrap();
    assert_eq!(
      calls.load(Ordering::SeqCst),
      1,
      "second call at the same generation within ttl reuses the cache"
    );
  }

  #[tokio::test]
  async fn new_generation_refetches() {
    let cache = RootBrowseCache::default();
    let calls = AtomicU32::new(0);
    cache
      .get_or_fetch(0, LONG_TTL, || async {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok::<_, ()>(result())
      })
      .await
      .unwrap();
    cache
      .get_or_fetch(1, LONG_TTL, || async {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok::<_, ()>(result())
      })
      .await
      .unwrap();
    assert_eq!(
      calls.load(Ordering::SeqCst),
      2,
      "a new generation invalidates and refetches"
    );
  }

  #[tokio::test(start_paused = true)]
  async fn expired_ttl_refetches_same_generation() {
    let cache = RootBrowseCache::default();
    let calls = AtomicU32::new(0);
    let ttl = Duration::from_secs(60);
    let fetch = || {
      cache.get_or_fetch(0, ttl, || async {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok::<_, ()>(result())
      })
    };
    fetch().await.unwrap();
    fetch().await.unwrap();
    assert_eq!(
      calls.load(Ordering::SeqCst),
      1,
      "within ttl, same generation, reuses cache"
    );

    tokio::time::advance(Duration::from_secs(61)).await;
    fetch().await.unwrap();
    assert_eq!(
      calls.load(Ordering::SeqCst),
      2,
      "past ttl at the same generation forces a refetch (recommendation/library drift backstop)"
    );
  }
}
