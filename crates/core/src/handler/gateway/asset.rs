use libbridgething::gateway::{AssetClear, AssetGotReply, GatewayToBridgeAssetMsgEventDispatch, TransferBody};
use tokio_util::bytes::Bytes;

use super::{HandlerResult, MsgHandle};
use crate::{
  asset::{AssetCache, Retention, wait::ASSET_STREAM_TIMEOUT},
  transfer::sinks::TransferSinks,
};

#[derive(Debug)]
pub struct AssetHandler {
  handle: MsgHandle,
}

impl AssetHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }
}

impl GatewayToBridgeAssetMsgEventDispatch for AssetHandler {
  type Output = HandlerResult;

  async fn clear(&self, params: AssetClear) -> HandlerResult {
    tracing::debug!(id = %params.id, "({:?}) asset clear from gateway", &self.handle.address);
    if let Err(err) = self.handle.state.assets.clear(&params.id).await {
      tracing::warn!(?err, id = %params.id, "asset clear failed");
    }
    Ok(())
  }
}

pub(crate) async fn cache_late_asset(assets: AssetCache, sinks: TransferSinks, reply: AssetGotReply) {
  let AssetGotReply { id, mime, body } = reply;
  let bytes = match body {
    TransferBody::Inline(bytes) => Bytes::from(bytes),
    TransferBody::Stream(transfer) => {
      match sinks
        .collect_memory(transfer.id, transfer.total_size, ASSET_STREAM_TIMEOUT)
        .await
      {
        Some(bytes) => bytes,
        None => {
          tracing::debug!(%id, "late asset stream reassembly failed or timed out; not caching");
          return;
        }
      }
    }
  };

  if bytes.is_empty() {
    tracing::debug!(%id, "late asset reply was empty; not caching");
    return;
  }

  if let Err(err) = assets
    .insert_internal(id.clone(), bytes, mime, Retention::DISK_LRU)
    .await
  {
    tracing::warn!(?err, %id, "failed to cache late asset reply");
  } else {
    tracing::debug!(%id, "cached late asset reply past its request timeout");
  }
}

#[cfg(test)]
mod tests {
  use libbridgething::gateway::TransferRef;
  use uuid::Uuid;

  use super::*;

  async fn fresh_cache() -> (AssetCache, TransferSinks, tokio::task::JoinHandle<()>) {
    let db = crate::db::open(None).await.unwrap();
    let blobs = std::env::temp_dir().join(format!("bridgething-late-asset-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&blobs).unwrap();
    let (assets, join) = AssetCache::init(db, blobs).await.unwrap().spawn();
    (assets, TransferSinks::default(), join)
  }

  #[tokio::test]
  async fn inline_late_reply_lands_in_cache() {
    let (assets, sinks, _join) = fresh_cache().await;
    let reply = AssetGotReply {
      id: "spotify/img/248/inline".into(),
      mime: Some("image/jpeg".into()),
      body: TransferBody::Inline(b"jpeg-bytes".to_vec()),
    };
    cache_late_asset(assets.clone(), sinks, reply).await;
    let got = assets.get("spotify/img/248/inline").await.unwrap().expect("cached");
    assert_eq!(&got.bytes[..], b"jpeg-bytes");
    assert_eq!(got.mime.as_deref(), Some("image/jpeg"));
  }

  #[tokio::test]
  async fn streamed_late_reply_reassembles_and_caches() {
    let (assets, sinks, _join) = fresh_cache().await;
    let transfer_id = Uuid::now_v7();
    let payload: &[u8] = b"streamed-art-bytes";
    // mirror the inbound loop: the sink is bound synchronously before the terminal reply is handled.
    sinks.bind_memory(transfer_id);
    let reply = AssetGotReply {
      id: "spotify/img/248/stream".into(),
      mime: Some("image/jpeg".into()),
      body: TransferBody::Stream(TransferRef {
        id: transfer_id,
        total_size: payload.len() as u32,
        sha256: None,
      }),
    };
    let task = {
      let assets = assets.clone();
      let sinks = sinks.clone();
      tokio::spawn(async move { cache_late_asset(assets, sinks, reply).await })
    };
    // fragments follow the terminal reply on the wire, in offset order.
    sinks.fragment(transfer_id, 0, Bytes::copy_from_slice(payload));
    task.await.unwrap();
    let got = assets.get("spotify/img/248/stream").await.unwrap().expect("cached");
    assert_eq!(&got.bytes[..], payload);
  }

  #[tokio::test]
  async fn empty_inline_reply_is_not_cached() {
    let (assets, sinks, _join) = fresh_cache().await;
    let reply = AssetGotReply {
      id: "spotify/img/248/empty".into(),
      mime: None,
      body: TransferBody::Inline(Vec::new()),
    };
    cache_late_asset(assets.clone(), sinks, reply).await;
    assert!(assets.get("spotify/img/248/empty").await.unwrap().is_none());
  }
}
