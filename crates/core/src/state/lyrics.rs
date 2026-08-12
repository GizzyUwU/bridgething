use std::{future::Future, num::NonZeroUsize};

use libbridgething::{Lyrics, gateway::TrackIdentity};

use super::cache::GenerationCache;

const MAX_ENTRIES: NonZeroUsize = NonZeroUsize::new(1).expect("nonzero cap");

#[derive(Debug)]
pub struct LyricsCache {
  inner: GenerationCache<TrackIdentity, Option<Lyrics>>,
}

impl Default for LyricsCache {
  fn default() -> Self {
    Self {
      inner: GenerationCache::new(MAX_ENTRIES),
    }
  }
}

impl LyricsCache {
  pub async fn get_or_fetch<F, Fut, E>(&self, key: &TrackIdentity, fetch: F) -> Result<Option<Lyrics>, E>
  where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Option<Lyrics>, E>>,
  {
    self.inner.get_or_fetch(key.clone(), None, None, fetch).await
  }
}

#[cfg(test)]
mod tests {
  use std::sync::atomic::{AtomicU32, Ordering};

  use super::*;

  fn identity(track: &str) -> TrackIdentity {
    TrackIdentity {
      artist: "artist".into(),
      track: track.into(),
      album: None,
      duration_ms: None,
      isrc: None,
    }
  }

  fn lyrics() -> Lyrics {
    Lyrics {
      synced: None,
      plain: Some("la".into()),
      source: "test".into(),
    }
  }

  #[tokio::test]
  async fn a_second_ask_for_the_same_track_does_not_refetch() {
    let cache = LyricsCache::default();
    let calls = AtomicU32::new(0);
    let fetch = || async {
      calls.fetch_add(1, Ordering::SeqCst);
      Ok::<_, ()>(Some(lyrics()))
    };

    assert!(cache.get_or_fetch(&identity("a"), fetch).await.unwrap().is_some());
    assert!(cache.get_or_fetch(&identity("a"), fetch).await.unwrap().is_some());

    assert_eq!(calls.load(Ordering::SeqCst), 1);
  }

  #[tokio::test]
  async fn a_track_with_no_lyrics_is_not_retried() {
    let cache = LyricsCache::default();
    let calls = AtomicU32::new(0);
    let fetch = || async {
      calls.fetch_add(1, Ordering::SeqCst);
      Ok::<_, ()>(None)
    };

    assert!(cache.get_or_fetch(&identity("a"), fetch).await.unwrap().is_none());
    assert!(cache.get_or_fetch(&identity("a"), fetch).await.unwrap().is_none());

    assert_eq!(calls.load(Ordering::SeqCst), 1);
  }

  #[tokio::test(start_paused = true)]
  async fn a_stalled_fetch_does_not_block_a_different_track() {
    let cache = LyricsCache::default();
    let (stalled, other) = (identity("a"), identity("b"));
    let slow = cache.get_or_fetch(&stalled, || async {
      tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
      Ok::<_, ()>(Some(lyrics()))
    });
    let fast = tokio::time::timeout(
      tokio::time::Duration::from_millis(50),
      cache.get_or_fetch(&other, || async { Ok::<_, ()>(Some(lyrics())) }),
    );
    let (_, fast) = tokio::join!(slow, fast);
    assert!(
      fast.is_ok(),
      "a stalled companion fetch must not stall a lookup for another track"
    );
  }

  #[tokio::test]
  async fn a_new_track_evicts_the_previous_entry() {
    let cache = LyricsCache::default();
    let calls = AtomicU32::new(0);
    let fetch = || async {
      calls.fetch_add(1, Ordering::SeqCst);
      Ok::<_, ()>(Some(lyrics()))
    };

    cache.get_or_fetch(&identity("a"), fetch).await.unwrap();
    cache.get_or_fetch(&identity("b"), fetch).await.unwrap();
    cache.get_or_fetch(&identity("a"), fetch).await.unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 3);
  }
}
