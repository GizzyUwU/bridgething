use std::time::Duration;

use libbridgething::{
  client::ClientAssetCommand,
  gateway::{AssetRequest, BridgeToGatewayAssetMsg, BridgeToGatewayMsg, BridgeToGatewayMsgData, GatewayMsgMeta},
  server::{AssetGot, AssetNotFound, ServerAssetEvent},
};
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use super::{HandlerResult, MsgHandle};
use crate::{
  asset::AssetCacheEvent,
  bluetooth::{GatewayMessage, GatewayType},
};

/// How long the daemon waits after issuing an `AssetRequest` to the
/// companion before giving up and returning `NotFound`. Long enough to
/// cover BT round-trip plus the companion's encode time, short enough
/// that webapps don't hang their UI.
const ASSET_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub struct AssetHandler {
  handle: MsgHandle,
}

impl AssetHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(&self, msg: ClientAssetCommand) -> HandlerResult {
    match msg {
      ClientAssetCommand::Get { id, request_id } => self.handle_get(id, request_id).await,
    }
  }

  async fn handle_get(&self, id: String, request_id: Uuid) -> HandlerResult {
    if let Some(asset) = self.handle.state.assets.get(&id).await? {
      return self
        .handle
        .respond(ServerAssetEvent::Got(AssetGot {
          request_id,
          id,
          bytes: asset.bytes.to_vec(),
          mime: asset.mime,
        }))
        .await
        .map_err(Into::into);
    }

    if !self.handle.state.gateway_status().await.connected {
      return self
        .handle
        .respond(ServerAssetEvent::NotFound(AssetNotFound { request_id, id }))
        .await
        .map_err(Into::into);
    }

    let mut events_rx = self.handle.state.assets.subscribe();
    self.send_asset_request(&id, request_id).await;

    let resolved = tokio::time::timeout(ASSET_REQUEST_TIMEOUT, async {
      loop {
        match events_rx.recv().await {
          Ok(AssetCacheEvent::Ready { id: ready }) if ready == id => return Some(()),
          Ok(_) => continue,
          Err(RecvError::Lagged(_)) => continue,
          Err(RecvError::Closed) => return None,
        }
      }
    })
    .await;

    if resolved.is_ok()
      && let Some(asset) = self.handle.state.assets.get(&id).await?
    {
      return self
        .handle
        .respond(ServerAssetEvent::Got(AssetGot {
          request_id,
          id,
          bytes: asset.bytes.to_vec(),
          mime: asset.mime,
        }))
        .await
        .map_err(Into::into);
    }

    self
      .handle
      .respond(ServerAssetEvent::NotFound(AssetNotFound { request_id, id }))
      .await
      .map_err(Into::into)
  }

  async fn send_asset_request(&self, id: &str, request_id: Uuid) {
    let payload = BridgeToGatewayMsgData::Asset(BridgeToGatewayAssetMsg::Request(AssetRequest {
      id: id.to_string(),
      request_id,
    }));
    self
      .handle
      .bluetooth
      .gateway_man
      .send_all(GatewayMessage::new(
        None,
        GatewayType::Rfcomm,
        BridgeToGatewayMsg {
          id: Uuid::now_v7(),
          meta: GatewayMsgMeta::Request,
          data: payload,
        },
      ))
      .await;
  }
}
