use libbridgething::gateway::GatewayToBridgeGeoMsg;

use super::{HandlerResult, MsgHandle};

pub struct GeoHandler {
  handle: MsgHandle,
}

impl GeoHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: GatewayToBridgeGeoMsg) -> HandlerResult {
    match msg {
      GatewayToBridgeGeoMsg::Position(_) => self.handle.unimplemented("gateway:geo.position").await,
      GatewayToBridgeGeoMsg::GetOnceReply(_) => self.handle.unimplemented("gateway:geo.getOnceReply").await,
      GatewayToBridgeGeoMsg::ErrorReply(_) => self.handle.unimplemented("gateway:geo.errorReply").await,
    }
    Ok(())
  }
}
