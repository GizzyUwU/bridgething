use std::{sync::LazyLock, time::Duration};

use libbridgething::{
  client::{
    AssetGet, AssetGot as WireAssetGot, AssetNotFound as WireAssetNotFound, AssetPreload, BridgeToClientAssetMsg,
    ClientToBridgeAssetMsgDispatch,
  },
  gateway::{AssetRequest, TransferBody},
  wire::RequestError,
};
use tokio::sync::Semaphore;
use tokio_util::bytes::Bytes;
use uuid::Uuid;

use super::{HandlerResult, MsgHandle};
use crate::{
  asset::{
    CachedAsset, Retention,
    wait::{ASSET_WAIT_TIMEOUT, FetchOutcome, wait_for_asset},
  },
  bluetooth::BluetoothMan,
  state::State,
  transfer::sinks::TransferSinks,
};

const PRELOAD_IDS_MAX: usize = 64;
const PRELOAD_PARALLELISM: usize = 16;
const IAP2_ART_PREFIX: &str = "iap2/art/";
const ASSET_STREAM_TIMEOUT: Duration = Duration::from_secs(30);
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
    match resolve_asset(&self.handle.state, &self.handle.bluetooth, &id).await {
      FetchOutcome::Got(asset) => self.respond_got(request_id, id, asset).await,
      FetchOutcome::NotFound => self.respond_not_found(request_id, id).await,
    }
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

pub(crate) async fn resolve_asset(state: &State, bluetooth: &BluetoothMan, id: &str) -> FetchOutcome {
  match state.assets.get(id).await {
    Ok(Some(asset)) => return FetchOutcome::Got(asset),
    Ok(None) => {}
    Err(err) => {
      tracing::warn!(?err, %id, "asset cache get failed");
      return FetchOutcome::NotFound;
    }
  }

  if id.starts_with(IAP2_ART_PREFIX) {
    fetch_iap2_art(state, id).await
  } else if state.gateway_info().is_none() {
    FetchOutcome::NotFound
  } else {
    fetch_via_companion(state, bluetooth, id).await
  }
}

async fn fetch_iap2_art(state: &State, id: &str) -> FetchOutcome {
  if !state.iap2_pending_art.is_pending(id).await {
    return FetchOutcome::NotFound;
  }
  let cache = state.assets.clone();
  let id_owned = id.to_string();
  state
    .asset_wait
    .fetch_or_wait(id, move || async move {
      match wait_for_asset(&cache, &id_owned, ASSET_WAIT_TIMEOUT).await {
        Some(asset) => FetchOutcome::Got(asset),
        None => FetchOutcome::NotFound,
      }
    })
    .await
}

pub(crate) async fn request_asset_body(
  sinks: &TransferSinks,
  bluetooth: &BluetoothMan,
  id: &str,
) -> Option<(Bytes, Option<String>)> {
  let request_id = Uuid::now_v7();
  // bind before sending so fragments racing ahead of the terminal reply are not dropped
  sinks.bind_memory(request_id);
  let req = AssetRequest {
    id: id.to_string(),
    request_id,
  };
  match bluetooth.gateway_man.request(None, req).await {
    Ok(got) => match got.body {
      TransferBody::Inline(bytes) => {
        sinks.unbind(request_id);
        Some((Bytes::from(bytes), got.mime))
      }
      TransferBody::Stream(transfer) => {
        if transfer.id != request_id {
          tracing::warn!(%id, %request_id, ref_id = %transfer.id, "asset reply ref id does not match request id");
          sinks.unbind(request_id);
          return None;
        }
        match sinks
          .collect_memory(request_id, transfer.total_size, ASSET_STREAM_TIMEOUT)
          .await
        {
          Some(bytes) => Some((bytes, got.mime)),
          None => {
            tracing::warn!(%id, "asset fragment reassembly failed or timed out");
            None
          }
        }
      }
    },
    Err(RequestError::Domain(_)) => {
      sinks.unbind(request_id);
      tracing::debug!(%id, "companion reported asset not found");
      None
    }
    Err(err) => {
      sinks.unbind(request_id);
      tracing::warn!(?err, %id, "asset request failed");
      None
    }
  }
}

async fn fetch_via_companion(state: &State, bluetooth: &BluetoothMan, id: &str) -> FetchOutcome {
  let cache = state.assets.clone();
  let sinks = state.transfer_sinks.clone();
  let bluetooth = bluetooth.clone();
  let id_owned = id.to_string();
  state
    .asset_wait
    .fetch_or_wait(id, move || async move {
      match request_asset_body(&sinks, &bluetooth, &id_owned).await {
        Some((bytes, mime)) => {
          if let Err(err) = cache
            .insert_internal(id_owned.clone(), bytes.clone(), mime.clone(), Retention::DISK_LRU)
            .await
          {
            tracing::warn!(?err, "failed to insert daemon-fetched asset into cache");
          }
          FetchOutcome::Got(CachedAsset { bytes, mime })
        }
        None => FetchOutcome::NotFound,
      }
    })
    .await
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
      let sinks = state.transfer_sinks.clone();
      let id_owned = id.clone();
      let _ = state
        .asset_wait
        .fetch_or_wait(&id, move || async move {
          let _permit = PRELOAD_GATE.acquire().await;
          match request_asset_body(&sinks, &bluetooth, &id_owned).await {
            Some((bytes, mime)) => {
              if let Err(err) = cache
                .insert_internal(id_owned.clone(), bytes.clone(), mime.clone(), Retention::DISK_LRU)
                .await
              {
                tracing::warn!(?err, %id_owned, "preload: failed to insert into cache");
              }
              FetchOutcome::Got(CachedAsset { bytes, mime })
            }
            None => FetchOutcome::NotFound,
          }
        })
        .await;
    });
  }
}
