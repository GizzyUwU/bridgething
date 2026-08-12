use std::{collections::HashMap, sync::Arc};

use bridgething_gateway::{HandlerError, LibraryHandler, OutboundLink, OutboundLinkExt, Reply};
use libbridgething::{
  LibraryError, Priority,
  gateway::{
    BrowseReply, ContextResolveReply, FavoritesContainsReply, FavoritesListReply, FavoritesSet, FavoritesSetMany,
    FavoritesToggle, GatewayToBridgeLibraryMsgEvent, LibraryBrowseRequest, LibraryErrorReply,
    LibraryFavoritesContainsRequest, LibraryFavoritesListRequest, LibraryRecommendationsRequest, LibrarySearchRequest,
    RecommendationsReply, SearchReply,
  },
  protocol::Compress,
  wire::WireError,
};

use crate::provider::{Provider, ProviderError, ProviderRegistry};

fn is_root(node_id: Option<&str>) -> bool {
  matches!(node_id, None | Some("") | Some("root"))
}

pub struct LibraryDispatcher {
  providers: Arc<dyn ProviderRegistry>,
  link: Arc<dyn OutboundLink>,
}

impl LibraryDispatcher {
  pub fn new(providers: Arc<dyn ProviderRegistry>, link: Arc<dyn OutboundLink>) -> Self {
    Self { providers, link }
  }

  fn active(&self) -> Result<Arc<dyn Provider>, HandlerError<LibraryErrorReply>> {
    self.providers.library().ok_or_else(|| {
      HandlerError::Domain(LibraryErrorReply {
        error: LibraryError::NotSupported {
          reason: "no active music provider".into(),
        },
      })
    })
  }

  fn owner(&self, uri: &str) -> Option<Arc<dyn Provider>> {
    self.providers.for_uri(uri).or_else(|| self.providers.library())
  }

  async fn report(&self, error: LibraryError) {
    if let Err(failure) = self
      .link
      .event(GatewayToBridgeLibraryMsgEvent::ErrorEvent(LibraryErrorReply { error }))
      .await
    {
      tracing::warn!(?failure, "the library error event did not reach the peer");
    }
  }

  async fn report_write(&self, error: ProviderError) {
    match refusal(error) {
      HandlerError::Domain(reply) => self.report(reply.error).await,
      HandlerError::Wire(_) => {
        self
          .report(LibraryError::NotSupported {
            reason: "unimplemented".into(),
          })
          .await
      }
    }
  }
}

fn refusal(error: ProviderError) -> HandlerError<LibraryErrorReply> {
  let error = match error {
    ProviderError::NotImplemented => return HandlerError::Wire(WireError::Unimplemented),
    ProviderError::NotAuthenticated => LibraryError::Unauthorized,
    ProviderError::Detached => LibraryError::NotSupported {
      reason: "the music provider detached".into(),
    },
    ProviderError::Failed(reason) => LibraryError::NotSupported { reason },
  };
  HandlerError::Domain(LibraryErrorReply { error })
}

impl LibraryHandler for LibraryDispatcher {
  async fn browse(&self, request: LibraryBrowseRequest) -> Result<Reply<BrowseReply>, HandlerError<LibraryErrorReply>> {
    let root = is_root(request.node_id.as_deref());
    let result = self.active()?.browse(request).await.map_err(refusal)?;
    let reply = Reply::new(BrowseReply { result });
    Ok(if root {
      reply.lane(Priority::Bulk).compressed(Compress::Always)
    } else {
      reply
    })
  }

  async fn resolve_context(
    &self,
    request: libbridgething::gateway::LibraryResolveContextRequest,
  ) -> Result<Reply<ContextResolveReply>, HandlerError<LibraryErrorReply>> {
    let resolved = self.active()?.resolve_context(&request.uri).await.map_err(refusal)?;
    Ok(resolved.into())
  }

  async fn search(&self, request: LibrarySearchRequest) -> Result<Reply<SearchReply>, HandlerError<LibraryErrorReply>> {
    let result = self.active()?.search(request).await.map_err(refusal)?;
    Ok(SearchReply { result }.into())
  }

  async fn recommendations(
    &self,
    request: LibraryRecommendationsRequest,
  ) -> Result<Reply<RecommendationsReply>, HandlerError<LibraryErrorReply>> {
    let result = self.active()?.recommendations(request).await.map_err(refusal)?;
    Ok(RecommendationsReply { result }.into())
  }

  async fn favorites_list(
    &self,
    request: LibraryFavoritesListRequest,
  ) -> Result<Reply<FavoritesListReply>, HandlerError<LibraryErrorReply>> {
    let page = self.active()?.favorites_list(request).await.map_err(refusal)?;
    Ok(FavoritesListReply { page }.into())
  }

  async fn favorites_contains(
    &self,
    request: LibraryFavoritesContainsRequest,
  ) -> Result<Reply<FavoritesContainsReply>, HandlerError<LibraryErrorReply>> {
    let liked = self.active()?.favorites_contains(request).await.map_err(refusal)?;
    Ok(FavoritesContainsReply { liked }.into())
  }

  async fn favorites_toggle(&self, payload: FavoritesToggle) -> Result<(), WireError> {
    let Some(provider) = self.owner(&payload.item.uri) else {
      self.report(LibraryError::NoGateway).await;
      return Ok(());
    };
    if let Err(error) = provider.favorites_toggle(payload.item).await {
      self.report_write(error).await;
    }
    Ok(())
  }

  async fn favorites_set(&self, payload: FavoritesSet) -> Result<(), WireError> {
    let Some(provider) = self.owner(&payload.item.uri) else {
      self.report(LibraryError::NoGateway).await;
      return Ok(());
    };
    if let Err(error) = provider.favorites_set(payload.item, payload.liked).await {
      self.report_write(error).await;
    }
    Ok(())
  }

  async fn favorites_set_many(&self, payload: FavoritesSetMany) -> Result<(), WireError> {
    let mut grouped: HashMap<String, (Arc<dyn Provider>, Vec<FavoritesSet>)> = HashMap::new();
    for entry in payload.entries {
      let Some(provider) = self.owner(&entry.item.uri) else {
        self.report(LibraryError::NoGateway).await;
        continue;
      };
      grouped
        .entry(provider.name().to_owned())
        .or_insert_with(|| (provider, Vec::new()))
        .1
        .push(entry);
    }

    for (provider, entries) in grouped.into_values() {
      if let Err(error) = provider.favorites_set_many(entries).await {
        self.report_write(error).await;
      }
    }
    Ok(())
  }
}
