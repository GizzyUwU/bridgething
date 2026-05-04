use libbridgething::{
  AssetRetention,
  client::{
    AssetGet, AssetGot as WireAssetGot, AssetNotFound as WireAssetNotFound, AssetPreload, BridgeToClientAssetMsg,
    ClientToBridgeAssetMsg,
  },
  gateway::AssetRequest,
  wire::RequestError,
};
use tokio_util::bytes::Bytes;
use uuid::Uuid;

use super::{HandlerResult, MsgHandle};

const PRELOAD_IDS_MAX: usize = 64;

#[derive(Debug)]
pub struct AssetHandler {
  handle: MsgHandle,
}

impl AssetHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(&self, msg: ClientToBridgeAssetMsg) -> HandlerResult {
    match msg {
      ClientToBridgeAssetMsg::Get(AssetGet { id, request_id }) => self.handle_get(id, request_id).await,
      ClientToBridgeAssetMsg::Preload(AssetPreload { ids }) => {
        self.handle_preload(ids).await;
        Ok(())
      }
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

  async fn handle_preload(&self, ids: Vec<String>) {
    if self.handle.state.gateway_info().await.is_none() {
      return;
    }
    for id in ids.into_iter().take(PRELOAD_IDS_MAX) {
      if matches!(self.handle.state.assets.get(&id).await, Ok(Some(_))) {
        continue;
      }
      let state = self.handle.state.clone();
      let bluetooth = self.handle.bluetooth.clone();
      tokio::spawn(async move {
        let req = AssetRequest {
          id: id.clone(),
          request_id: Uuid::now_v7(),
        };
        match bluetooth.gateway_man.request(None, req).await {
          Ok(got) => {
            let bytes = Bytes::from(got.bytes);
            if let Err(err) = state
              .assets
              .insert(id.clone(), bytes, got.mime, AssetRetention::Lru)
              .await
            {
              tracing::warn!(?err, %id, "preload: failed to insert into cache");
            }
          }
          Err(RequestError::Domain(_)) => {
            tracing::debug!(%id, "preload: companion reported asset not found");
          }
          Err(err) => {
            tracing::debug!(?err, %id, "preload: companion request failed");
          }
        }
      });
    }
  }
}
