use libbridgething::client::{
  ClientToBridgeWebappMsgRequest, WebappError, WebappErrorReply, WebappIcon, WebappIconReply,
};

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
      ClientToBridgeWebappMsgRequest::Icon(WebappIcon { id }) => {
        match self.handle.state.webapps.read_icon(id).await {
          Some((bytes, mime)) => {
            self
              .handle
              .respond_to::<WebappIcon>(WebappIconReply { bytes, mime })
              .await?;
          }
          None => {
            self
              .handle
              .respond_err::<WebappIcon>(WebappErrorReply {
                error: WebappError::IconNotAvailable { id: id.to_string() },
              })
              .await?;
          }
        }
        Ok(())
      }
    }
  }
}
