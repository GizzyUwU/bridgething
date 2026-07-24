use std::{collections::HashMap, future::Future};

use libbridgething::BrowseResult;
use tokio::{
  sync::Mutex,
  time::{Duration, Instant},
};

pub type RootBrowseShape = (Option<u32>, Option<u32>);

#[derive(Debug)]
struct Cached {
  generation: u64,
  fetched_at: Instant,
  result: BrowseResult,
}

#[derive(Debug, Default)]
pub struct RootBrowseCache {
  inner: Mutex<HashMap<RootBrowseShape, Cached>>,
}

impl RootBrowseCache {
  pub async fn get_or_fetch<F, Fut, E>(
    &self,
    shape: RootBrowseShape,
    generation: u64,
    ttl: Duration,
    fetch: F,
  ) -> Result<BrowseResult, E>
  where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<BrowseResult, E>>,
  {
    let mut guard = self.inner.lock().await;
    if let Some(cached) = guard.get(&shape)
      && cached.generation == generation
      && cached.fetched_at.elapsed() < ttl
    {
      return Ok(cached.result.clone());
    }
    let result = fetch().await?;
    guard.retain(|_, cached| cached.generation == generation);
    guard.insert(
      shape,
      Cached {
        generation,
        fetched_at: Instant::now(),
        result: result.clone(),
      },
    );
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
  const FULL: RootBrowseShape = (None, None);
  const SLIM: RootBrowseShape = (Some(10), None);

  #[tokio::test]
  async fn same_generation_within_ttl_reuses_cache() {
    let cache = RootBrowseCache::default();
    let calls = AtomicU32::new(0);
    let fetch = || {
      cache.get_or_fetch(FULL, 0, LONG_TTL, || async {
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
  async fn distinct_shapes_cache_independently() {
    let cache = RootBrowseCache::default();
    let calls = AtomicU32::new(0);
    let fetch = |shape| {
      cache.get_or_fetch(shape, 0, LONG_TTL, || async {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok::<_, ()>(result())
      })
    };
    fetch(FULL).await.unwrap();
    fetch(SLIM).await.unwrap();
    fetch(FULL).await.unwrap();
    fetch(SLIM).await.unwrap();
    assert_eq!(
      calls.load(Ordering::SeqCst),
      2,
      "each request shape fetches once and then hits its own entry"
    );
  }

  #[tokio::test]
  async fn new_generation_refetches() {
    let cache = RootBrowseCache::default();
    let calls = AtomicU32::new(0);
    cache
      .get_or_fetch(FULL, 0, LONG_TTL, || async {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok::<_, ()>(result())
      })
      .await
      .unwrap();
    cache
      .get_or_fetch(FULL, 1, LONG_TTL, || async {
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

  #[tokio::test]
  async fn new_generation_drops_every_shape() {
    let cache = RootBrowseCache::default();
    let calls = AtomicU32::new(0);
    let fetch = |shape, generation| {
      cache.get_or_fetch(shape, generation, LONG_TTL, || async {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok::<_, ()>(result())
      })
    };
    fetch(FULL, 0).await.unwrap();
    fetch(SLIM, 0).await.unwrap();
    fetch(FULL, 1).await.unwrap();
    fetch(SLIM, 1).await.unwrap();
    assert_eq!(
      calls.load(Ordering::SeqCst),
      4,
      "a generation bump invalidates stale entries of every shape"
    );
  }

  #[tokio::test(start_paused = true)]
  async fn expired_ttl_refetches_same_generation() {
    let cache = RootBrowseCache::default();
    let calls = AtomicU32::new(0);
    let ttl = Duration::from_secs(60);
    let fetch = || {
      cache.get_or_fetch(FULL, 0, ttl, || async {
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
