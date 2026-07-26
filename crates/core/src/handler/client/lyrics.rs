use libbridgething::{
  MediaItem,
  client::{ClientToBridgeLyricsMsgDispatch, LyricsError, LyricsErrorReply, LyricsGet, LyricsReply},
  gateway::{LyricsRequest, TrackIdentity},
  wire::RequestError,
};

use super::{HandlerResult, MsgHandle};

pub struct LyricsHandler {
  handle: MsgHandle,
}

impl LyricsHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  async fn respond_error(&self, error: LyricsError) -> HandlerResult {
    self
      .handle
      .respond_err::<LyricsGet>(LyricsErrorReply { error })
      .await
      .map_err(Into::into)
  }
}

impl ClientToBridgeLyricsMsgDispatch for LyricsHandler {
  type Output = HandlerResult;

  async fn get(&self) -> HandlerResult {
    let caps = self.handle.state.capabilities.snapshot();
    if caps.gateway.is_none() {
      return self.respond_error(LyricsError::NoGateway).await;
    }
    if !caps.available.lyrics {
      return self.respond_error(LyricsError::NotSupported).await;
    }

    let Some(track) = self.handle.state.player.state_reply().state.track else {
      return self.respond_error(LyricsError::NothingPlaying).await;
    };
    let track_uri = track.uri.clone();
    let track_persistent_id = track.persistent_id.clone();
    let Some(identity) = track_identity(track) else {
      return self.respond_error(LyricsError::TrackUnidentifiable).await;
    };

    let gateway_man = &self.handle.bluetooth.gateway_man;
    let outbound = LyricsRequest {
      track: identity.clone(),
    };
    let fetched = self
      .handle
      .state
      .lyrics
      .get_or_fetch(&identity, || async move {
        gateway_man.request(None, outbound).await.map(|reply| reply.lyrics)
      })
      .await;

    match fetched {
      Ok(lyrics) => self
        .handle
        .respond_to::<LyricsGet>(LyricsReply {
          track_uri,
          track_persistent_id,
          lyrics,
        })
        .await
        .map_err(Into::into),
      Err(RequestError::Domain(domain)) => {
        self
          .respond_error(LyricsError::LookupFailed { reason: domain.message })
          .await
      }
      Err(RequestError::Protocol(err)) => {
        tracing::warn!(?err, "lyrics.get protocol error");
        self
          .respond_error(LyricsError::LookupFailed {
            reason: "the gateway could not be reached".into(),
          })
          .await
      }
      Err(RequestError::ResponseMismatch) => {
        tracing::error!("lyrics.get response did not match expected shape");
        self
          .respond_error(LyricsError::LookupFailed {
            reason: "the gateway replied with an unexpected shape".into(),
          })
          .await
      }
    }
  }
}

fn track_identity(track: MediaItem) -> Option<TrackIdentity> {
  Some(TrackIdentity {
    artist: track.artist?,
    track: track.title?,
    album: track.album,
    duration_ms: track.duration_ms,
    isrc: None,
  })
}
