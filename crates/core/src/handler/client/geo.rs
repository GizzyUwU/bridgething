use libbridgething::client::ClientToBridgeGeoMsg;

use super::{HandlerResult, MsgHandle};

pub struct GeoHandler {
  handle: MsgHandle,
}

impl GeoHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: ClientToBridgeGeoMsg) -> HandlerResult {
    match msg {
      ClientToBridgeGeoMsg::Watch(_) => Ok(self.handle.unimplemented("geo.watch").await?),
      ClientToBridgeGeoMsg::Unwatch(_) => Ok(self.handle.unimplemented("geo.unwatch").await?),
      ClientToBridgeGeoMsg::GetOnce(_) => Ok(self.handle.unimplemented("geo.getOnce").await?),
    }
  }
}
