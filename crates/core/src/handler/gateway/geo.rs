use libbridgething::{client::BridgeToClientGeoMsgEvent, gateway::GatewayToBridgeGeoMsgEvent};

use super::{HandlerResult, MsgHandle};

pub struct GeoHandler {
  handle: MsgHandle,
}

impl GeoHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: GatewayToBridgeGeoMsgEvent) -> HandlerResult {
    match msg {
      GatewayToBridgeGeoMsgEvent::Position(position) => {
        let owners = self.handle.state.geo_watchers.owners();
        if owners.is_empty() {
          tracing::trace!("geo position arrived with no watchers; dropping");
          return Ok(());
        }
        for owner in owners {
          let event = BridgeToClientGeoMsgEvent::Position(position);
          if let Err(err) = self.handle.state.bus.send_event(owner, event).await {
            tracing::warn!(?err, %owner, "failed to forward geo position to webapp");
          }
        }
      }
    }
    Ok(())
  }
}
