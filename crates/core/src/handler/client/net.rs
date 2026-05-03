use libbridgething::client::ClientToBridgeNetMsg;

use super::{HandlerResult, MsgHandle};

pub struct NetHandler {
  handle: MsgHandle,
}

impl NetHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: ClientToBridgeNetMsg) -> HandlerResult {
    match msg {
      ClientToBridgeNetMsg::Fetch(_) => Ok(self.handle.unimplemented("net.fetch").await?),
      ClientToBridgeNetMsg::WsOpen(_) => Ok(self.handle.unimplemented("net.wsOpen").await?),
      ClientToBridgeNetMsg::WsClose(_) => Ok(self.handle.unimplemented("net.wsClose").await?),
      ClientToBridgeNetMsg::WsSend(_) => Ok(self.handle.unimplemented("net.wsSend").await?),
    }
  }
}
