use libbridgething::client::{
  ClientToBridgeWebappMsgRequest, WebappActivate, WebappActiveReply, WebappCurrent, WebappCurrentReply, WebappError,
  WebappErrorReply, WebappIcon, WebappIconReply, WebappList, WebappListReply,
};

use super::{HandlerResult, MsgHandle};
use crate::{chrome::ChromeCommand, handler::gateway::webapp::navigate_url_for_active};

pub struct WebappHandler {
  handle: MsgHandle,
}

impl WebappHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: ClientToBridgeWebappMsgRequest) -> HandlerResult {
    match msg {
      ClientToBridgeWebappMsgRequest::List => self.list().await,
      ClientToBridgeWebappMsgRequest::Current => self.current().await,
      ClientToBridgeWebappMsgRequest::Activate(req) => self.activate(req).await,
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

  async fn list(&self) -> HandlerResult {
    let webapps = self.handle.state.webapps.list_for_clients().await;
    self
      .handle
      .respond_to::<WebappList>(WebappListReply { webapps })
      .await?;
    Ok(())
  }

  async fn current(&self) -> HandlerResult {
    let id = self.handle.state.active_webapp().await?;
    let name = match id {
      Some(id) => self
        .handle
        .state
        .webapps
        .bundle(id)
        .await
        .map(|b| b.manifest.name.clone()),
      None => None,
    };
    self
      .handle
      .respond_to::<WebappCurrent>(WebappCurrentReply { id, name })
      .await?;
    Ok(())
  }

  async fn activate(&self, req: WebappActivate) -> HandlerResult {
    let WebappActivate { id } = req;
    if self.handle.state.webapps.resolve(id).await.is_none() {
      self.handle.state.webapps.rescan().await;
    }
    if self.handle.state.webapps.resolve(id).await.is_none() {
      self
        .handle
        .respond_err::<WebappActivate>(WebappErrorReply {
          error: WebappError::WebappNotFound { id: id.to_string() },
        })
        .await?;
      return Ok(());
    }

    self.handle.state.set_active_webapp(id).await?;
    let url = navigate_url_for_active(&self.handle.state).await;
    if let Err(e) = self.handle.state.chrome.send(ChromeCommand::Navigate(url)).await {
      tracing::warn!("failed to reload kiosk after webapp activate: {:?}", e);
    }
    let name = self
      .handle
      .state
      .webapps
      .bundle(id)
      .await
      .map(|b| b.manifest.name.clone());
    self
      .handle
      .respond_to::<WebappActivate>(WebappActiveReply { id: Some(id), name })
      .await?;
    Ok(())
  }
}
