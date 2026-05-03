use libbridgething::{
  AssetRetention,
  client::{
    AssetGet, AssetGot as WireAssetGot, AssetNotFound as WireAssetNotFound, BridgeToClientAssetMsg,
    ClientToBridgeAssetMsgRequest,
  },
  gateway::AssetRequest,
  wire::RequestError,
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

  pub async fn handle(&self, msg: ClientToBridgeAssetMsgRequest) -> HandlerResult {
    match msg {
      ClientToBridgeAssetMsgRequest::Get(AssetGet { id, request_id }) => self.handle_get(id, request_id).await,
    }
  }

  async fn handle_get(&self, id: String, request_id: Uuid) -> HandlerResult {
    if let Some(asset) = self.handle.state.assets.get(&id).await? {
      return self
        .handle
        .respond(BridgeToClientAssetMsg::Got(WireAssetGot {
          request_id,
          id,
          bytes: asset.bytes.to_vec(),
          mime: asset.mime,
        }))
        .await
        .map_err(Into::into);
    }

    if self.handle.state.gateway_info().await.is_none() {
      return self
        .handle
        .respond(BridgeToClientAssetMsg::NotFound(WireAssetNotFound { request_id, id }))
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
        // One Bytes view shared between the cache insert and the wire
        // send. Bytes::clone() is a refcount bump; the heap-allocation
        // for the asset bytes happens once, when rmp_serde::from_slice
        // built `got.bytes`. The wire send below converts that single
        // allocation back into a Vec via `into()`.
        let bytes = Bytes::from(got.bytes);
        if let Err(err) = self
          .handle
          .state
          .assets
          .insert(id.clone(), bytes.clone(), got.mime.clone(), AssetRetention::Lru)
          .await
        {
          tracing::warn!(?err, "failed to insert daemon-fetched asset into cache");
        }
        self
          .handle
          .respond(BridgeToClientAssetMsg::Got(WireAssetGot {
            request_id,
            id: got.id,
            bytes: bytes.into(),
            mime: got.mime,
          }))
          .await
          .map_err(Into::into)
      }
      Err(RequestError::Domain(nf)) => {
        tracing::debug!(id = %nf.id, "companion reported asset not found");
        self
          .handle
          .respond(BridgeToClientAssetMsg::NotFound(WireAssetNotFound {
            request_id,
            id: nf.id,
          }))
          .await
          .map_err(Into::into)
      }
      Err(RequestError::Protocol(err)) => {
        tracing::warn!(?err, %id, "asset request failed at protocol level");
        self
          .handle
          .respond(BridgeToClientAssetMsg::NotFound(WireAssetNotFound { request_id, id }))
          .await
          .map_err(Into::into)
      }
      Err(RequestError::ResponseMismatch) => {
        tracing::error!(%id, "asset response did not match expected shape");
        self
          .handle
          .respond(BridgeToClientAssetMsg::NotFound(WireAssetNotFound { request_id, id }))
          .await
          .map_err(Into::into)
      }
    }
  }
}
