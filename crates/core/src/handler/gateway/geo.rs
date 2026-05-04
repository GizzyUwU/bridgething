use libbridgething::gateway::GatewayToBridgeGeoMsgEvent;

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
      GatewayToBridgeGeoMsgEvent::Position(_) => self.handle.unimplemented("gateway:geo.position").await,
    }
    Ok(())
  }
}
