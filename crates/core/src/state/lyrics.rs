use std::future::Future;

use libbridgething::{Lyrics, gateway::TrackIdentity};
use tokio::sync::Mutex;

#[derive(Debug, Default)]
pub struct LyricsCache {
  inner: Mutex<Option<Entry>>,
}

#[derive(Debug)]
struct Entry {
  key: TrackIdentity,
  lyrics: Option<Lyrics>,
}

impl LyricsCache {
  pub async fn get_or_fetch<F, Fut, E>(&self, key: &TrackIdentity, fetch: F) -> Result<Option<Lyrics>, E>
  where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Option<Lyrics>, E>>,
  {
    let mut guard = self.inner.lock().await;
    if let Some(entry) = guard.as_ref()
      && &entry.key == key
    {
      return Ok(entry.lyrics.clone());
    }
    let lyrics = fetch().await?;
    *guard = Some(Entry {
      key: key.clone(),
      lyrics: lyrics.clone(),
    });
    Ok(lyrics)
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
