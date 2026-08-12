use std::{sync::Arc, time::Duration};

use bridgething_delivery::{
  seam::Clock,
  transfer::{AckWindow, BytesSource, LinkPush},
};
use bridgething_gateway::{AssetHandler, HandlerError, OutboundLink, OutboundLinkExt, Reply};
use libbridgething::gateway::{
  AssetGotReply, AssetNotFoundReply, AssetRequest, GatewayToBridgeTransferMsgEvent, TransferAbandon, TransferBody,
  TransferRef,
};
use uuid::Uuid;

use crate::provider::{AssetBytes, ProviderRegistry};

pub const ASSET_FRAGMENT_BYTES: usize = 4 * 1024;
pub const INLINE_BODY_MAX_BYTES: usize = 8 * 1024;
pub const ASSET_ACK_TIMEOUT: Duration = Duration::from_secs(15);

pub struct AssetDispatcher {
  providers: Arc<dyn ProviderRegistry>,
  push: LinkPush,
}

impl AssetDispatcher {
  pub fn new(providers: Arc<dyn ProviderRegistry>, link: Arc<dyn OutboundLink>, clock: Arc<dyn Clock>) -> Self {
    Self {
      providers,
      push: LinkPush::new(link, Arc::new(AckWindow::new()), clock),
    }
  }

  pub fn acks(&self) -> &Arc<AckWindow> {
    self.push.acks()
  }

  async fn push_stream(push: LinkPush, transfer_id: Uuid, bytes: Vec<u8>) {
    let source = BytesSource::new(bytes);
    let sent = push
      .run(
        transfer_id,
        &source,
        &source.whole(),
        0,
        ASSET_FRAGMENT_BYTES,
        ASSET_ACK_TIMEOUT,
      )
      .await;
    push.acks().finish(transfer_id);
    if let Err(reason) = sent {
      tracing::warn!(transfer = %transfer_id, %reason, "asset push gave up");
      let _ = push
        .link()
        .event(GatewayToBridgeTransferMsgEvent::Abandon(TransferAbandon {
          transfer_id,
          reason: reason.to_string(),
        }))
        .await;
    }
  }

  async fn resolve(&self, id: &str) -> Option<AssetBytes> {
    let owner = id.split(['/', ':']).next().unwrap_or_default();
    let providers = self.providers.all();
    let ordered = providers
      .iter()
      .filter(|provider| provider.name() == owner)
      .chain(providers.iter().filter(|provider| provider.name() != owner));

    for provider in ordered {
      match provider.asset(id).await {
        Ok(Some(bytes)) => return Some(bytes),
        Ok(None) => {}
        Err(reason) => tracing::warn!(%id, provider = provider.name(), %reason, "asset resolve failed"),
      }
    }
    None
  }
}

impl AssetHandler for AssetDispatcher {
  async fn request(&self, request: AssetRequest) -> Result<Reply<AssetGotReply>, HandlerError<AssetNotFoundReply>> {
    let Some(asset) = self.resolve(&request.id).await else {
      return Err(HandlerError::Domain(AssetNotFoundReply { id: request.id }));
    };

    if asset.bytes.len() <= INLINE_BODY_MAX_BYTES {
      return Ok(
        AssetGotReply {
          id: request.id,
          mime: asset.mime,
          body: TransferBody::Inline(asset.bytes),
        }
        .into(),
      );
    }

    let reference = TransferRef {
      id: request.request_id,
      total_size: asset.bytes.len() as u32,
      sha256: None,
    };
    let announced = AssetGotReply {
      id: request.id,
      mime: asset.mime,
      body: TransferBody::Stream(reference),
    };
    Ok(Reply::new(announced).then(Self::push_stream(self.push.clone(), request.request_id, asset.bytes)))
  }
}
