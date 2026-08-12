use std::sync::Arc;

use bridgething_io::{HttpExecutor, HttpMethod, HttpRequest};
use libbridgething::{Lyrics, gateway::TrackIdentity};

use crate::{dispatch::lyrics::LyricsResolver, lyrics::lrc};

const LOOKUP_TIMEOUT_MS: u32 = 10_000;

pub struct LrclibResolver {
  http: HttpExecutor,
  base: String,
}

impl LrclibResolver {
  pub fn new(http: HttpExecutor) -> Arc<Self> {
    Self::with_base(http, "https://lrclib.net")
  }

  pub fn with_base(http: HttpExecutor, base: &str) -> Arc<Self> {
    Arc::new(Self {
      http,
      base: base.trim_end_matches('/').to_owned(),
    })
  }

  fn lookup_url(&self, track: &TrackIdentity) -> Option<String> {
    if track.artist.is_empty() || track.track.is_empty() {
      return None;
    }
    let mut query: Vec<(&str, String)> = vec![
      ("artist_name", track.artist.clone()),
      ("track_name", track.track.clone()),
    ];
    if let Some(album) = track.album.as_ref().filter(|album| !album.is_empty()) {
      query.push(("album_name", album.clone()));
    }
    if let Some(ms) = track.duration_ms {
      query.push(("duration", (u64::from(ms) + 500).div_euclid(1000).to_string()));
    }
    let url = url::Url::parse_with_params(
      &format!("{}/api/get", self.base),
      query.iter().map(|(k, v)| (*k, v.as_str())),
    )
    .ok()?;
    Some(url.to_string())
  }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrclibHit {
  synced_lyrics: Option<String>,
  plain_lyrics: Option<String>,
}

#[async_trait::async_trait]
impl LyricsResolver for LrclibResolver {
  async fn lyrics(&self, track: &TrackIdentity) -> Option<Lyrics> {
    let url = self.lookup_url(track)?;
    let response = self
      .http
      .execute(HttpRequest {
        method: HttpMethod::Get,
        url,
        headers: Vec::new(),
        body: Vec::new(),
        timeout_ms: LOOKUP_TIMEOUT_MS,
      })
      .await
      .ok()?;
    if response.status != 200 {
      tracing::debug!(status = response.status, artist = %track.artist, track = %track.track, "lrclib lookup missed");
      return None;
    }
    let hit: LrclibHit = serde_json::from_slice(&response.body).ok()?;
    let synced = hit
      .synced_lyrics
      .as_deref()
      .map(lrc::parse)
      .filter(|lines| !lines.is_empty());
    let plain = hit.plain_lyrics.filter(|text| !text.is_empty());
    if synced.is_none() && plain.is_none() {
      return None;
    }
    Some(Lyrics {
      synced,
      plain,
      source: "lrclib".into(),
    })
  }
}
