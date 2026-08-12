use std::sync::Arc;

use bridgething_gateway::{HandlerError, LyricsHandler, Reply};
use libbridgething::{
  Lyrics,
  gateway::{LyricsErrorReply, LyricsReply, LyricsRequest, TrackIdentity},
};

use crate::provider::{ProviderError, ProviderRegistry};

#[async_trait::async_trait]
pub trait LyricsResolver: Send + Sync {
  async fn lyrics(&self, track: &TrackIdentity) -> Option<Lyrics>;
}

pub struct LyricsDispatcher {
  providers: Arc<dyn ProviderRegistry>,
  resolver: Arc<dyn LyricsResolver>,
}

impl LyricsDispatcher {
  pub fn new(providers: Arc<dyn ProviderRegistry>, resolver: Arc<dyn LyricsResolver>) -> Self {
    Self { providers, resolver }
  }
}

impl LyricsHandler for LyricsDispatcher {
  async fn get(&self, request: LyricsRequest) -> Result<Reply<LyricsReply>, HandlerError<LyricsErrorReply>> {
    let provider = self.providers.audible().or_else(|| self.providers.library());
    if let Some(provider) = provider {
      match provider.lyrics(&request.track).await {
        Ok(Some(lyrics)) => return Ok(LyricsReply { lyrics: Some(lyrics) }.into()),
        Ok(None) | Err(ProviderError::NotImplemented) => {}
        Err(reason) => {
          return Err(HandlerError::Domain(LyricsErrorReply {
            message: reason.to_string(),
          }));
        }
      }
    }

    Ok(
      LyricsReply {
        lyrics: self.resolver.lyrics(&request.track).await,
      }
      .into(),
    )
  }
}
