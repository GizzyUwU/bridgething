use libbridgething::{
  Position,
  client::BridgeToClientGeoMsgEvent,
  gateway::{GatewayToBridgeGeoMsgEventDispatch, GeoErrorReply},
};

use super::{HandlerResult, MsgHandle};

pub struct GeoHandler {
  handle: MsgHandle,
}

impl GeoHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  async fn fan_out(&self, what: &str, make: impl Fn() -> BridgeToClientGeoMsgEvent) -> HandlerResult {
    let owners = self.handle.state.geo_watchers.owners();
    if owners.is_empty() {
      tracing::trace!("geo {what} arrived with no watchers; dropping");
      return Ok(());
    }
    for owner in owners {
      if let Err(err) = self.handle.state.bus.send_event(owner, make()).await {
        tracing::warn!(?err, %owner, "failed to forward geo {what} to webapp");
      }
    }
    Ok(())
  }
}

impl GatewayToBridgeGeoMsgEventDispatch for GeoHandler {
  type Output = HandlerResult;

  async fn position(&self, params: Position) -> HandlerResult {
    self.handle.state.geo_last_fix.record(params);
    self
      .fan_out("position", || BridgeToClientGeoMsgEvent::Position(params))
      .await
  }

  async fn error_event(&self, params: GeoErrorReply) -> HandlerResult {
    tracing::warn!(error = ?params.error, "companion reported a geo watch failure");
    self
      .fan_out("error", || {
        BridgeToClientGeoMsgEvent::ErrorEvent(libbridgething::client::GeoErrorReply { error: params.error })
      })
      .await
  }
}
