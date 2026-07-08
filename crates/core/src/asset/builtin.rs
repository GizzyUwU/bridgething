//! Assets baked into the daemon binary, served without a companion round trip.
//!
//! Some cover art has no source url: Spotify renders the Liked Songs gradient
//! client-side, so its metadata carries no image. These ids resolve to embedded
//! bytes so the tile is never blank, regardless of whether a phone is attached.

use tokio_util::bytes::Bytes;

use super::CachedAsset;

pub const BUILTIN_ART_PREFIX: &str = "builtin/img/";

static LIKED_SONGS_WEBP: &[u8] =
  include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/resources/builtin/liked-songs.webp"));

pub fn lookup(id: &str) -> Option<CachedAsset> {
  let key = id.strip_prefix(BUILTIN_ART_PREFIX)?;
  let (bytes, mime) = match key {
    "liked-songs" => (LIKED_SONGS_WEBP, "image/webp"),
    _ => return None,
  };
  Some(CachedAsset {
    bytes: Bytes::from_static(bytes),
    mime: Some(mime.to_string()),
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn liked_songs_resolves_to_embedded_webp() {
    let asset = lookup("builtin/img/liked-songs").expect("liked songs builtin present");
    assert_eq!(asset.mime.as_deref(), Some("image/webp"));
    assert_eq!(asset.bytes.get(0..4), Some(&b"RIFF"[..]));
  }

  #[test]
  fn unknown_builtin_and_foreign_ids_miss() {
    assert!(lookup("builtin/img/nope").is_none());
    assert!(lookup("spotify/img/248/iabc").is_none());
  }
}
