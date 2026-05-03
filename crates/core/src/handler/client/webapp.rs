use libbridgething::client::ClientToBridgeWebappMsgRequest;

use super::{HandlerResult, MsgHandle};

pub struct WebappHandler {
  handle: MsgHandle,
}

impl WebappHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: ClientToBridgeWebappMsgRequest) -> HandlerResult {
    match msg {
      ClientToBridgeWebappMsgRequest::List => Ok(self.handle.unimplemented("webapp.list").await?),
      ClientToBridgeWebappMsgRequest::Current => Ok(self.handle.unimplemented("webapp.current").await?),
      ClientToBridgeWebappMsgRequest::Activate(_) => Ok(self.handle.unimplemented("webapp.activate").await?),
      ClientToBridgeWebappMsgRequest::Uninstall(_) => Ok(self.handle.unimplemented("webapp.uninstall").await?),
      ClientToBridgeWebappMsgRequest::Install(_) => Ok(self.handle.unimplemented("webapp.install").await?),
    }
  }
}
