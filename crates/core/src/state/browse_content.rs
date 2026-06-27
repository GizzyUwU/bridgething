use std::{future::Future, num::NonZeroUsize};

use libbridgething::BrowseResult;
use lru::LruCache;
use tokio::{
  sync::Mutex,
  time::{Duration, Instant},
};

const IMMUTABLE_TTL: Duration = Duration::from_secs(12 * 60 * 60);
const CATALOG_TTL: Duration = Duration::from_secs(60 * 60);
const MUTABLE_TTL: Duration = Duration::from_secs(120);
const MAX_ENTRIES: usize = 64;

#[derive(Clone, Copy)]
enum BrowseKind {
  Immutable,
  Catalog,
  Mutable,
}

impl BrowseKind {
  fn classify(node_id: &str) -> Self {
    if node_id.starts_with("spotify:album:") {
      Self::Immutable
    } else if node_id.starts_with("spotify:artist:") || node_id.starts_with("spotify:show:") {
      Self::Catalog
    } else {
      Self::Mutable
    }
  }

  fn ttl(self) -> Duration {
    match self {
      Self::Immutable => IMMUTABLE_TTL,
      Self::Catalog => CATALOG_TTL,
      Self::Mutable => MUTABLE_TTL,
    }
  }

  fn gen_keyed(self) -> bool {
    matches!(self, Self::Mutable)
  }
}

#[derive(Hash, PartialEq, Eq)]
struct Key {
  node_id: String,
  offset: u32,
  limit: u32,
}

struct Entry {
  generation: Option<u64>,
  expires_at: Instant,
  result: BrowseResult,
}

impl Entry {
  fn is_fresh(&self, generation: u64) -> bool {
    Instant::now() < self.expires_at && self.generation.is_none_or(|g| g == generation)
  }
}

#[derive(Debug)]
pub struct BrowseContentCache {
  inner: Mutex<LruCache<Key, Entry>>,
}

impl Default for BrowseContentCache {
  fn default() -> Self {
    Self {
      inner: Mutex::new(LruCache::new(NonZeroUsize::new(MAX_ENTRIES).expect("nonzero cap"))),
    }
  }
}

impl BrowseContentCache {
  pub async fn get_or_fetch<F, Fut, E>(
    &self,
    node_id: &str,
    offset: u32,
    limit: u32,
    generation: u64,
    fetch: F,
  ) -> Result<BrowseResult, E>
  where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<BrowseResult, E>>,
  {
    let kind = BrowseKind::classify(node_id);
    let key = Key {
      node_id: node_id.to_string(),
      offset,
      limit,
    };

    {
      let mut cache = self.inner.lock().await;
      let stale = match cache.get(&key) {
        Some(entry) if entry.is_fresh(generation) => return Ok(entry.result.clone()),
        Some(_) => true,
        None => false,
      };
      if stale {
        cache.pop(&key);
      }
    }

    let result = fetch().await?;
    let entry = Entry {
      generation: kind.gen_keyed().then_some(generation),
      expires_at: Instant::now() + kind.ttl(),
      result: result.clone(),
    };
    self.inner.lock().await.put(key, entry);
    Ok(result)
  }
}

#[cfg(test)]
mod tests {
  use std::sync::atomic::{AtomicU32, Ordering};

  use super::*;

  fn result(total: u32) -> BrowseResult {
    BrowseResult {
      entries: Vec::new(),
      total: Some(total),
      has_more: false,
    }
  }

  async fn fetch_count(cache: &BrowseContentCache, node_id: &str, generation: u64, calls: &AtomicU32) -> BrowseResult {
    cache
      .get_or_fetch(node_id, 0, 50, generation, || async {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok::<_, ()>(result(7))
      })
      .await
      .unwrap()
  }

  #[test]
  fn classify_by_uri_shape() {
    assert!(matches!(
      BrowseKind::classify("spotify:album:abc"),
      BrowseKind::Immutable
    ));
    assert!(matches!(
      BrowseKind::classify("spotify:artist:abc"),
      BrowseKind::Catalog
    ));
    assert!(matches!(BrowseKind::classify("spotify:show:abc"), BrowseKind::Catalog));
    assert!(matches!(
      BrowseKind::classify("spotify:playlist:abc"),
      BrowseKind::Mutable
    ));
    assert!(matches!(
      BrowseKind::classify("spotify:user:joey:collection"),
      BrowseKind::Mutable
    ));
    assert!(matches!(BrowseKind::classify("playlists"), BrowseKind::Mutable));
    assert!(matches!(BrowseKind::classify("recently-played"), BrowseKind::Mutable));
  }

  #[tokio::test]
  async fn same_key_within_ttl_reuses_cache() {
    let cache = BrowseContentCache::default();
    let calls = AtomicU32::new(0);
    fetch_count(&cache, "spotify:album:a", 0, &calls).await;
    fetch_count(&cache, "spotify:album:a", 0, &calls).await;
    assert_eq!(
      calls.load(Ordering::SeqCst),
      1,
      "second lookup of the same page hits the cache"
    );
  }

  #[tokio::test]
  async fn distinct_pages_are_separate_entries() {
    let cache = BrowseContentCache::default();
    let calls = AtomicU32::new(0);
    cache
      .get_or_fetch("spotify:album:a", 0, 50, 0, || async {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok::<_, ()>(result(7))
      })
      .await
      .unwrap();
    cache
      .get_or_fetch("spotify:album:a", 50, 50, 0, || async {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok::<_, ()>(result(7))
      })
      .await
      .unwrap();
    assert_eq!(
      calls.load(Ordering::SeqCst),
      2,
      "a different offset is a different cache key"
    );
  }

  #[tokio::test(start_paused = true)]
  async fn album_survives_reconnect_but_playlist_does_not() {
    let cache = BrowseContentCache::default();
    let calls = AtomicU32::new(0);

    // immutable album content ignores the generation: a reconnect (bumped gen) still hits the cache.
    fetch_count(&cache, "spotify:album:a", 0, &calls).await;
    fetch_count(&cache, "spotify:album:a", 1, &calls).await;
    assert_eq!(
      calls.load(Ordering::SeqCst),
      1,
      "album content is not keyed on the companion generation"
    );

    // mutable playlist content is gen-keyed: a reconnect invalidates and refetches.
    fetch_count(&cache, "spotify:playlist:p", 0, &calls).await;
    fetch_count(&cache, "spotify:playlist:p", 1, &calls).await;
    assert_eq!(
      calls.load(Ordering::SeqCst),
      3,
      "a new generation invalidates a playlist drilldown"
    );
  }

  #[tokio::test(start_paused = true)]
  async fn ttl_expiry_is_per_retention_class() {
    let cache = BrowseContentCache::default();
    let calls = AtomicU32::new(0);

    fetch_count(&cache, "spotify:playlist:p", 0, &calls).await;
    fetch_count(&cache, "spotify:artist:r", 0, &calls).await;
    assert_eq!(calls.load(Ordering::SeqCst), 2, "first lookups miss and fetch");

    // past the mutable ttl but inside the catalog ttl: only the playlist refetches.
    tokio::time::advance(MUTABLE_TTL + Duration::from_secs(1)).await;
    fetch_count(&cache, "spotify:playlist:p", 0, &calls).await;
    fetch_count(&cache, "spotify:artist:r", 0, &calls).await;
    assert_eq!(calls.load(Ordering::SeqCst), 3, "playlist expired, artist still fresh");

    // past the catalog ttl: the artist refetches too.
    tokio::time::advance(CATALOG_TTL).await;
    fetch_count(&cache, "spotify:artist:r", 0, &calls).await;
    assert_eq!(
      calls.load(Ordering::SeqCst),
      4,
      "artist content expired past its medium ttl"
    );
  }

  #[tokio::test]
  async fn lru_bound_evicts_least_recently_used() {
    let cache = BrowseContentCache::default();
    let calls = AtomicU32::new(0);

    for i in 0..MAX_ENTRIES {
      fetch_count(&cache, &format!("spotify:album:{i}"), 0, &calls).await;
    }
    assert_eq!(
      calls.load(Ordering::SeqCst),
      MAX_ENTRIES as u32,
      "cold-fill misses every entry"
    );

    // touch entry 0 so it becomes most-recently-used, then overflow by one to evict the true lru (1).
    fetch_count(&cache, "spotify:album:0", 0, &calls).await;
    assert_eq!(
      calls.load(Ordering::SeqCst),
      MAX_ENTRIES as u32,
      "entry 0 was still cached"
    );
    fetch_count(&cache, "spotify:album:overflow", 0, &calls).await;

    // entry 0 (recently touched) is retained; entry 1 (the lru) was evicted and refetches.
    fetch_count(&cache, "spotify:album:0", 0, &calls).await;
    assert_eq!(
      calls.load(Ordering::SeqCst),
      MAX_ENTRIES as u32 + 1,
      "the recently-used entry is retained"
    );
    fetch_count(&cache, "spotify:album:1", 0, &calls).await;
    assert_eq!(
      calls.load(Ordering::SeqCst),
      MAX_ENTRIES as u32 + 2,
      "the least-recently-used entry was evicted"
    );
  }
}
