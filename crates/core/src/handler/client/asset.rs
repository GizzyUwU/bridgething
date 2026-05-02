use libbridgething::{
  AssetRetention,
  client::ClientAssetCommand,
  gateway::{AssetRequest, RequestError},
  server::{AssetGot as WireAssetGot, AssetNotFound as WireAssetNotFound, ServerAssetEvent},
};
use tokio_util::bytes::Bytes;
use uuid::Uuid;

use super::{HandlerResult, MsgHandle};

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
        .respond(ServerAssetEvent::Got(WireAssetGot {
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
        .respond(ServerAssetEvent::NotFound(WireAssetNotFound { request_id, id }))
        .await
        .map_err(Into::into);
    }

    let req = AssetRequest {
      id: id.clone(),
      request_id: Uuid::now_v7(),
    };
    let response = self.handle.bluetooth.gateway_man.request(None, req).await;

    match response {
      Ok(got) => {
        if let Err(err) = self
          .handle
          .state
          .assets
          .insert(
            id.clone(),
            Bytes::from(got.bytes.clone()),
            got.mime.clone(),
            AssetRetention::Lru,
          )
          .await
        {
          tracing::warn!(?err, "failed to insert daemon-fetched asset into cache");
        }
        self
          .handle
          .respond(ServerAssetEvent::Got(WireAssetGot {
            request_id,
            id: got.id,
            bytes: got.bytes,
            mime: got.mime,
          }))
          .await
          .map_err(Into::into)
      }
      Err(RequestError::Domain(nf)) => {
        tracing::debug!(id = %nf.id, "companion reported asset not found");
        self
          .handle
          .respond(ServerAssetEvent::NotFound(WireAssetNotFound { request_id, id: nf.id }))
          .await
          .map_err(Into::into)
      }
      Err(RequestError::Protocol(err)) => {
        tracing::warn!(?err, %id, "asset request failed at protocol level");
        self
          .handle
          .respond(ServerAssetEvent::NotFound(WireAssetNotFound { request_id, id }))
          .await
          .map_err(Into::into)
      }
      Err(RequestError::ResponseMismatch) => {
        tracing::error!(%id, "asset response did not match expected shape");
        self
          .handle
          .respond(ServerAssetEvent::NotFound(WireAssetNotFound { request_id, id }))
          .await
          .map_err(Into::into)
      }
    }
  }
}
