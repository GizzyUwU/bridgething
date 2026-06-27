//! HTTP-Range proxy that translates libswupdate's delta-fetch traffic
//! into wire `OtaAssetRange` requests against the pinned companion.
//!
//! Lifecycle is owned by [`OtaActor`]: the proxy is in `Inactive` state
//! at daemon boot, becomes `Active { update_id, peer }` when an OTA
//! enters `Streaming`, and reverts to `Inactive` when the OTA terminates
//! (success-by-reboot, failure, or cancel). HTTP requests arriving while
//! `Inactive` (or for the wrong `update_id`) get `409 Conflict`, which
//! libswupdate surfaces as a clean install failure.
//!
//! Range bytes arrive either inline in the reply or as a fragment stream
//! routed through the transfer sink registry, and stream straight back
//! out the HTTP response body - never accumulated in a `Vec<u8>` on the
//! daemon side. The 426 MB usable RAM on Superbird is shared with
//! chromium; range responses can be hundreds of MB if libcurl asks for
//! a large coalesced range.
//!
//! Actor-with-mpsc concurrency: a single broker task owns `active` and
//! the in-flight id set, and HTTP handlers reach it through a cloneable
//! `RangeProxy` handle. Deactivation unbinds every in-flight sink, which
//! closes each HTTP body stream with an error.

use std::collections::HashSet;

use bluer::Address;
use tokio::{
  sync::{mpsc, oneshot},
  task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{bluetooth::BluetoothMan, transfer::sinks::TransferSinks};

mod server;

const BROKER_MAILBOX: usize = 64;

#[derive(Clone)]
pub struct RangeProxy {
  cmd_tx: mpsc::Sender<BrokerCmd>,
}

impl std::fmt::Debug for RangeProxy {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("RangeProxy").finish_non_exhaustive()
  }
}

pub struct RangeProxyHandle {
  pub proxy: RangeProxy,
  pub cancel: CancellationToken,
  _broker: JoinHandle<()>,
  _server: Option<JoinHandle<()>>,
}

impl RangeProxy {
  pub async fn spawn(bluetooth: BluetoothMan, sinks: TransferSinks, port: u16) -> RangeProxyHandle {
    let (cmd_tx, cmd_rx) = mpsc::channel(BROKER_MAILBOX);
    let cancel = CancellationToken::new();

    let broker = BrokerActor {
      cmd_rx,
      sinks: sinks.clone(),
      active: None,
      inflight: Default::default(),
    };
    let _broker = tokio::spawn(broker.run());

    let proxy = RangeProxy { cmd_tx };
    let _server = match server::spawn(proxy.clone(), bluetooth, sinks, port, cancel.clone()).await {
      Ok(handle) => Some(handle),
      Err(err) => {
        tracing::error!(
          ?err,
          "ota range proxy failed to bind 127.0.0.1:{port}; delta OTA over wire unavailable until restart",
        );
        None
      }
    };

    RangeProxyHandle {
      proxy,
      cancel,
      _broker,
      _server,
    }
  }

  pub async fn activate(&self, update_id: String, peer: Option<Address>) {
    if let Err(err) = self.cmd_tx.send(BrokerCmd::Activate { update_id, peer }).await {
      tracing::error!(?err, "range proxy mailbox closed; activate dropped");
    }
  }

  pub async fn deactivate(&self) {
    if let Err(err) = self.cmd_tx.send(BrokerCmd::Deactivate).await {
      tracing::error!(?err, "range proxy mailbox closed; deactivate dropped");
    }
  }

  pub(crate) async fn begin_range_active(&self, request_id: Uuid) -> Result<RangeBegin, BeginRangeError> {
    let (reply_tx, reply_rx) = oneshot::channel();
    self
      .cmd_tx
      .send(BrokerCmd::BeginRange {
        request_id,
        reply: reply_tx,
      })
      .await
      .map_err(|_| BeginRangeError::ProxyDown)?;
    reply_rx.await.map_err(|_| BeginRangeError::ProxyDown)?
  }

  pub(crate) async fn end_range(&self, request_id: Uuid) {
    let _ = self.cmd_tx.send(BrokerCmd::EndRange { request_id }).await;
  }
}

#[derive(Debug, Clone)]
pub(crate) struct RangeBegin {
  pub update_id: String,
  pub peer: Option<Address>,
}

#[derive(Debug)]
pub(crate) enum BeginRangeError {
  NoActiveOta,
  ProxyDown,
}

#[derive(Debug)]
enum BrokerCmd {
  Activate {
    update_id: String,
    peer: Option<Address>,
  },
  Deactivate,
  BeginRange {
    request_id: Uuid,
    reply: oneshot::Sender<Result<RangeBegin, BeginRangeError>>,
  },
  EndRange {
    request_id: Uuid,
  },
}

#[derive(Debug, Clone)]
struct ActiveOta {
  update_id: String,
  peer: Option<Address>,
}

struct BrokerActor {
  cmd_rx: mpsc::Receiver<BrokerCmd>,
  sinks: TransferSinks,
  active: Option<ActiveOta>,
  inflight: HashSet<Uuid>,
}

impl BrokerActor {
  async fn run(mut self) {
    tracing::info!("ota range proxy broker started");
    while let Some(cmd) = self.cmd_rx.recv().await {
      match cmd {
        BrokerCmd::Activate { update_id, peer } => {
          tracing::info!(%update_id, ?peer, "range proxy activated");
          self.active = Some(ActiveOta { update_id, peer });
        }
        BrokerCmd::Deactivate => {
          if let Some(active) = self.active.take() {
            tracing::info!(update_id = %active.update_id, "range proxy deactivated");
          }
          for request_id in self.inflight.drain() {
            self.sinks.unbind(request_id);
          }
        }
        BrokerCmd::BeginRange { request_id, reply } => {
          let result = match &self.active {
            None => Err(BeginRangeError::NoActiveOta),
            Some(active) => {
              self.inflight.insert(request_id);
              Ok(RangeBegin {
                update_id: active.update_id.clone(),
                peer: active.peer,
              })
            }
          };
          let _ = reply.send(result);
        }
        BrokerCmd::EndRange { request_id } => {
          self.inflight.remove(&request_id);
          self.sinks.unbind(request_id);
        }
      }
    }
    tracing::info!("ota range proxy broker exiting");
  }
}

/// Test seam: a `RangeProxy` that drops every command. Used by the
/// orchestrator unit tests where the proxy lifecycle isn't exercised.
#[cfg(test)]
pub fn noop_proxy() -> RangeProxy {
  let (cmd_tx, mut cmd_rx) = mpsc::channel::<BrokerCmd>(16);
  tokio::spawn(async move { while cmd_rx.recv().await.is_some() {} });
  RangeProxy { cmd_tx }
}

#[cfg(test)]
mod tests {
  use tokio_util::bytes::Bytes;

  use super::*;
  use crate::transfer::sinks::TransferEvent;

  fn spawn_broker_only(sinks: TransferSinks) -> RangeProxy {
    let (cmd_tx, cmd_rx) = mpsc::channel(16);
    let broker = BrokerActor {
      cmd_rx,
      sinks,
      active: None,
      inflight: Default::default(),
    };
    tokio::spawn(broker.run());
    RangeProxy { cmd_tx }
  }

  #[tokio::test]
  async fn begin_range_with_no_active_returns_no_active_ota() {
    let proxy = spawn_broker_only(TransferSinks::default());
    let result = proxy.begin_range_active(Uuid::now_v7()).await;
    assert!(matches!(result, Err(BeginRangeError::NoActiveOta)));
  }

  #[tokio::test]
  async fn begin_range_returns_active_update_id_and_peer() {
    let proxy = spawn_broker_only(TransferSinks::default());
    proxy.activate("expected-id".into(), None).await;
    let begin = proxy
      .begin_range_active(Uuid::now_v7())
      .await
      .expect("begin should succeed");
    assert_eq!(begin.update_id, "expected-id");
    assert!(begin.peer.is_none());
  }

  #[tokio::test]
  async fn deactivate_unbinds_inflight_sinks() {
    let sinks = TransferSinks::default();
    let proxy = spawn_broker_only(sinks.clone());
    proxy.activate("a".into(), None).await;
    let req_id = Uuid::now_v7();
    let mut rx = sinks.bind_forward(req_id);
    proxy.begin_range_active(req_id).await.unwrap();

    sinks.fragment(req_id, 0, Bytes::from_static(b"x"));
    assert!(matches!(rx.recv().await, Some(TransferEvent::Fragment { .. })));

    proxy.deactivate().await;
    let res = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
      .await
      .expect("recv should resolve once broker unbinds");
    assert!(res.is_none(), "expected channel closed, got event: {res:?}");
  }
}
