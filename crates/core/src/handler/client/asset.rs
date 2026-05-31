use std::sync::LazyLock;

use libbridgething::{
  AssetRetention,
  client::{
    AssetGet, AssetGot as WireAssetGot, AssetNotFound as WireAssetNotFound, AssetPreload, BridgeToClientAssetMsg,
    ClientToBridgeAssetMsgDispatch,
  },
  gateway::AssetRequest,
  wire::RequestError,
};
use tokio::sync::Semaphore;
use tokio_util::bytes::Bytes;
use uuid::Uuid;

use super::{HandlerResult, MsgHandle};
use crate::{
  asset::{
    CachedAsset,
    wait::{ASSET_WAIT_TIMEOUT, FetchOutcome, wait_for_asset},
  },
  bluetooth::BluetoothMan,
  state::State,
};

const PRELOAD_IDS_MAX: usize = 64;
const PRELOAD_PARALLELISM: usize = 16;
const IAP2_ART_PREFIX: &str = "iap2/art/";
static PRELOAD_GATE: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(PRELOAD_PARALLELISM));

#[derive(Debug)]
pub struct AssetHandler {
  handle: MsgHandle,
}

impl AssetHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }
}

impl ClientToBridgeAssetMsgDispatch for AssetHandler {
  type Output = HandlerResult;

  async fn get(&self, params: AssetGet) -> HandlerResult {
    let AssetGet { id, request_id } = params;
    self.handle_get(id, request_id).await
  }

  async fn preload(&self, params: AssetPreload) -> HandlerResult {
    self.handle_preload(params.ids).await;
    Ok(())
  }
}

impl AssetHandler {
  async fn handle_get(&self, id: String, request_id: Uuid) -> HandlerResult {
    if let Some(asset) = self.handle.state.assets.get(&id).await? {
      return self.respond_got(request_id, id, asset).await;
    }

    let outcome = if id.starts_with(IAP2_ART_PREFIX) {
      self.fetch_iap2_art(&id).await
    } else if self.handle.state.gateway_info().is_none() {
      FetchOutcome::NotFound
    } else {
      self.fetch_via_companion(&id).await
    };

    match outcome {
      FetchOutcome::Got(asset) => self.respond_got(request_id, id, asset).await,
      FetchOutcome::NotFound => self.respond_not_found(request_id, id).await,
    }
  }

  async fn fetch_iap2_art(&self, id: &str) -> FetchOutcome {
    if !self.handle.state.iap2_pending_art.is_pending(id).await {
      return FetchOutcome::NotFound;
    }
    let cache = self.handle.state.assets.clone();
    let id_owned = id.to_string();
    self
      .handle
      .state
      .asset_wait
      .fetch_or_wait(id, move || async move {
        match wait_for_asset(&cache, &id_owned, ASSET_WAIT_TIMEOUT).await {
          Some(asset) => FetchOutcome::Got(asset),
          None => FetchOutcome::NotFound,
        }
      })
      .await
  }

  async fn fetch_via_companion(&self, id: &str) -> FetchOutcome {
    let cache = self.handle.state.assets.clone();
    let bluetooth = self.handle.bluetooth.clone();
    let id_owned = id.to_string();
    self
      .handle
      .state
      .asset_wait
      .fetch_or_wait(id, move || async move {
        let req = AssetRequest {
          id: id_owned.clone(),
          request_id: Uuid::now_v7(),
        };
        match bluetooth.gateway_man.request(None, req).await {
          Ok(got) => {
            let bytes = Bytes::from(got.bytes);
            if let Err(err) = cache
              .insert(id_owned.clone(), bytes.clone(), got.mime.clone(), AssetRetention::Lru)
              .await
            {
              tracing::warn!(?err, "failed to insert daemon-fetched asset into cache");
            }
            FetchOutcome::Got(CachedAsset {
              bytes,
              mime: got.mime,
              retention: AssetRetention::Lru,
            })
          }
          Err(RequestError::Domain(nf)) => {
            tracing::debug!(id = %nf.id, "companion reported asset not found");
            FetchOutcome::NotFound
          }
          Err(RequestError::Protocol(err)) => {
            tracing::warn!(?err, %id_owned, "asset request failed at protocol level");
            FetchOutcome::NotFound
          }
          Err(RequestError::ResponseMismatch) => {
            tracing::error!(%id_owned, "asset response did not match expected shape");
            FetchOutcome::NotFound
          }
        }
      })
      .await
  }

  async fn respond_got(&self, request_id: Uuid, id: String, asset: CachedAsset) -> HandlerResult {
    self
      .handle
      .respond(BridgeToClientAssetMsg::Got(WireAssetGot {
        request_id,
        id,
        bytes: asset.bytes.to_vec(),
        mime: asset.mime,
      }))
      .await
      .map_err(Into::into)
  }

  async fn respond_not_found(&self, request_id: Uuid, id: String) -> HandlerResult {
    self
      .handle
      .respond(BridgeToClientAssetMsg::NotFound(WireAssetNotFound { request_id, id }))
      .await
      .map_err(Into::into)
  }

  async fn handle_preload(&self, ids: Vec<String>) {
    preload_assets(self.handle.state.clone(), self.handle.bluetooth.clone(), ids).await;
  }
}

pub(crate) async fn preload_assets(state: State, bluetooth: BluetoothMan, ids: Vec<String>) {
  if state.gateway_info().is_none() {
    return;
  }
  for id in ids.into_iter().take(PRELOAD_IDS_MAX) {
    if id.starts_with(IAP2_ART_PREFIX) {
      continue;
    }
    if matches!(state.assets.get(&id).await, Ok(Some(_))) {
      continue;
    }
    let state = state.clone();
    let bluetooth = bluetooth.clone();
    tokio::spawn(async move {
      let cache = state.assets.clone();
      let id_owned = id.clone();
      let _ = state
        .asset_wait
        .fetch_or_wait(&id, move || async move {
          let _permit = PRELOAD_GATE.acquire().await;
          let req = AssetRequest {
            id: id_owned.clone(),
            request_id: Uuid::now_v7(),
          };
          match bluetooth.gateway_man.request(None, req).await {
            Ok(got) => {
              let bytes = Bytes::from(got.bytes);
              if let Err(err) = cache
                .insert(id_owned.clone(), bytes.clone(), got.mime.clone(), AssetRetention::Lru)
                .await
              {
                tracing::warn!(?err, %id_owned, "preload: failed to insert into cache");
              }
              FetchOutcome::Got(CachedAsset {
                bytes,
                mime: got.mime,
                retention: AssetRetention::Lru,
              })
            }
            Err(RequestError::Domain(_)) => {
              tracing::debug!(%id_owned, "preload: companion reported asset not found");
              FetchOutcome::NotFound
            }
            Err(err) => {
              tracing::debug!(?err, %id_owned, "preload: companion request failed");
              FetchOutcome::NotFound
            }
          }
        })
        .await;
    });
  }
}
