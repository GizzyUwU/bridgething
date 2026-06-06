use std::future::Future;

use libbridgething::BrowseResult;
use tokio::sync::Mutex;

#[derive(Debug)]
struct Cached {
  generation: u64,
  result: BrowseResult,
}

#[derive(Debug, Default)]
pub struct RootBrowseCache {
  inner: Mutex<Option<Cached>>,
}

impl RootBrowseCache {
  pub async fn get_or_fetch<F, Fut, E>(&self, generation: u64, fetch: F) -> Result<BrowseResult, E>
  where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<BrowseResult, E>>,
  {
    let mut guard = self.inner.lock().await;
    if let Some(cached) = guard.as_ref()
      && cached.generation == generation
    {
      return Ok(cached.result.clone());
    }
    let result = fetch().await?;
    *guard = Some(Cached {
      generation,
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

  #[tokio::test]
  async fn same_generation_reuses_cache() {
    let cache = RootBrowseCache::default();
    let calls = AtomicU32::new(0);
    let fetch = || {
      cache.get_or_fetch(0, || async {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok::<_, ()>(result())
      })
    };
    fetch().await.unwrap();
    fetch().await.unwrap();
    assert_eq!(
      calls.load(Ordering::SeqCst),
      1,
      "second call at the same generation reuses the cache"
    );
  }

  #[tokio::test]
  async fn new_generation_refetches() {
    let cache = RootBrowseCache::default();
    let calls = AtomicU32::new(0);
    cache
      .get_or_fetch(0, || async {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok::<_, ()>(result())
      })
      .await
      .unwrap();
    cache
      .get_or_fetch(1, || async {
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
}
