use std::time::Duration;

use libbridgething::{
  GeoError,
  client::{
    BridgeToClientMsgData, ClientToBridgeGeoMsgDispatch, GeoErrorReply, GeoGetOnce, GeoGetOnceReply, GeoUnwatch,
    GeoWatch, GeoWatchReply,
  },
  gateway::{self, BridgeToGatewayGeoMsgCommand},
  wire::{RequestError, WireRequest},
};
use uuid::Uuid;

use super::{HandlerResult, MsgHandle};
use crate::state::{GEO_PERMISSION, WatchAggregate, WatchChange};

pub struct GeoHandler {
  handle: MsgHandle,
}

impl GeoHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }
}

impl ClientToBridgeGeoMsgDispatch for GeoHandler {
  type Output = HandlerResult;

  async fn watch(&self, params: GeoWatch) -> HandlerResult {
    let GeoWatch {
      accuracy,
      min_interval_ms,
    } = params;
    if !self.declares_geo().await {
      return self.respond_error::<GeoWatch>(GeoError::NotDeclared).await;
    }
    if !self.has_geo() {
      return self.respond_error::<GeoWatch>(GeoError::Unavailable).await;
    }
    let token = Uuid::now_v7();
    let change = self
      .handle
      .state
      .geo_watchers
      .register(token, self.handle.from, accuracy, min_interval_ms);
    self.dispatch_aggregate_change(change).await;
    self
      .handle
      .respond_to::<GeoWatch>(GeoWatchReply {
        token: token.to_string(),
      })
      .await?;
    Ok(())
  }

  async fn unwatch(&self, params: GeoUnwatch) -> HandlerResult {
    let GeoUnwatch { token } = params;
    let Ok(uuid) = Uuid::parse_str(&token) else {
      tracing::trace!(%token, "geo.unwatch with malformed token; dropping");
      return Ok(());
    };
    let change = self.handle.state.geo_watchers.unregister(uuid);
    if !change.existed {
      tracing::trace!(%token, "geo.unwatch with unknown token; dropping");
      return Ok(());
    }
    self.dispatch_aggregate_change(change).await;
    Ok(())
  }

  async fn get_once(&self, params: GeoGetOnce) -> HandlerResult {
    let GeoGetOnce { accuracy, max_age_s } = params;
    if !self.declares_geo().await {
      return self.respond_error::<GeoGetOnce>(GeoError::NotDeclared).await;
    }
    if !self.has_geo() {
      return self.respond_error::<GeoGetOnce>(GeoError::Unavailable).await;
    }
    if let Some(max_age_s) = max_age_s
      && let Some(position) = self
        .handle
        .state
        .geo_last_fix
        .fresher_than(Duration::from_secs(max_age_s.into()))
    {
      tracing::trace!(max_age_s, "geo.getOnce answered from the held fix; phone not woken");
      self
        .handle
        .respond_to::<GeoGetOnce>(GeoGetOnceReply { position })
        .await?;
      return Ok(());
    }
    let outbound = gateway::GeoGetOnce { accuracy };
    let primary = self.handle.state.capabilities.primary_addr();
    match self.handle.bluetooth.gateway_man.request(primary, outbound).await {
      Ok(reply) => {
        self.handle.state.geo_last_fix.record(reply.position);
        self
          .handle
          .respond_to::<GeoGetOnce>(GeoGetOnceReply {
            position: reply.position,
          })
          .await?;
      }
      Err(err) => self.respond_request_error::<GeoGetOnce>("geo.getOnce", err).await?,
    }
    Ok(())
  }
}

impl GeoHandler {
  async fn declares_geo(&self) -> bool {
    self.handle.state.active_webapp_has_permission(GEO_PERMISSION).await
  }

  fn has_geo(&self) -> bool {
    let snapshot = self.handle.state.capabilities.snapshot();
    snapshot.gateway.is_some() && snapshot.available.geo
  }

  async fn dispatch_aggregate_change(&self, change: WatchChange) {
    dispatch_change(&self.handle, change).await
  }

  async fn respond_error<R>(&self, error: GeoError) -> HandlerResult
  where
    R: WireRequest<Inbound = BridgeToClientMsgData, DomainError = GeoErrorReply>,
  {
    self
      .handle
      .respond_err::<R>(GeoErrorReply { error })
      .await
      .map_err(Into::into)
  }

  async fn respond_request_error<R>(&self, verb: &str, err: RequestError<gateway::GeoErrorReply>) -> HandlerResult
  where
    R: WireRequest<Inbound = BridgeToClientMsgData, DomainError = GeoErrorReply>,
  {
    let error = match err {
      RequestError::Domain(domain) => domain.error,
      RequestError::Protocol(err) => {
        tracing::warn!(?err, "{verb} protocol error");
        GeoError::Unavailable
      }
      RequestError::ResponseMismatch => {
        tracing::error!("{verb} response did not match expected shape");
        GeoError::Unavailable
      }
    };
    self.respond_error::<R>(error).await
  }
}

pub async fn cleanup_owner_watchers(handle: &MsgHandle) {
  let change = handle.state.geo_watchers.drain_for_owner(handle.from);
  if !change.existed {
    return;
  }
  dispatch_change(handle, change).await;
}

async fn dispatch_change(handle: &MsgHandle, change: WatchChange) {
  if change.prev == change.next {
    return;
  }
  match change.next {
    Some(WatchAggregate {
      accuracy,
      min_interval_ms,
    }) => {
      handle
        .bluetooth
        .gateway_man
        .broadcast_command(BridgeToGatewayGeoMsgCommand::Watch(gateway::GeoWatch {
          accuracy,
          min_interval_ms,
        }))
        .await
    }
    None => {
      handle
        .bluetooth
        .gateway_man
        .broadcast_command(BridgeToGatewayGeoMsgCommand::Unwatch)
        .await
    }
  }
}
