//! Webapp control commands. Currently exposes `switch-webapp <uuid>`,
//! which sends `WebappSwitchTo` over the gateway WS. The daemon's
//! switch handler does a disk rescan if the id isn't in the registry,
//! so this command also handles the dev-iter "rsync a fresh bundle
//! then activate it" flow without needing a separate install request.

use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use libbridgething::{
  gateway::{GatewayToBridgeMsg, WebappSwitchTo},
  wire::{MsgMeta, RequestError, ResponseMeta, WireRequest},
};
use uuid::Uuid;

use crate::{
  chaos::ChaosConfig,
  conn::{Connection, OutboundFrame},
};

pub async fn run_switch(url: &str, chaos: ChaosConfig, id: Uuid) -> Result<()> {
  let mut conn = Connection::open(url, chaos).await?;
  conn.announce_version().await?;

  let request_id = Uuid::now_v7();
  let req = WebappSwitchTo { id };
  conn
    .outbound_tx
    .send(OutboundFrame::normal(GatewayToBridgeMsg {
      id: request_id,
      meta: MsgMeta::Request,
      data: req.into(),
    }))
    .await
    .map_err(|_| anyhow!("connection writer closed before WebappSwitchTo"))?;

  let deadline = Instant::now() + Duration::from_secs(15);
  loop {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
      return Err(anyhow!("WebappSwitchTo response timed out"));
    }
    let recv = tokio::time::timeout(remaining, conn.inbound_rx.recv()).await;
    let msg = match recv {
      Ok(Some(msg)) => msg,
      Ok(None) => return Err(anyhow!("connection closed before WebappSwitchTo response")),
      Err(_) => return Err(anyhow!("WebappSwitchTo response timed out")),
    };
    if let MsgMeta::Response(ResponseMeta { request_id: rid }) = &msg.meta
      && rid == &request_id
    {
      return match WebappSwitchTo::extract(msg.data) {
        Ok(active) => {
          tracing::info!(
            id = %active.id.map(|i| i.to_string()).unwrap_or_default(),
            name = active.name.as_deref().unwrap_or("(none)"),
            "switched active webapp"
          );
          Ok(())
        }
        Err(RequestError::Domain(err)) => Err(anyhow!("daemon rejected switch: {err:?}")),
        Err(RequestError::Protocol(err)) => Err(anyhow!("WebappSwitchTo protocol error: {err:?}")),
        Err(RequestError::ResponseMismatch) => Err(anyhow!("WebappSwitchTo response shape mismatch")),
      };
    }
    tracing::trace!(?msg, "non-response inbound during WebappSwitchTo wait");
  }
}
