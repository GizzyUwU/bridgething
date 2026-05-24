use libbridgething::{
  WebappError,
  client::{
    ClientToBridgeWebappMsgDispatch, WebappActivate, WebappActiveReply, WebappCurrent, WebappCurrentReply, WebappIcon,
    WebappIconReply, WebappInstallAbandon, WebappInstallBegin, WebappInstallBeginAck, WebappInstallChunk, WebappList,
    WebappListReply,
  },
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
}

impl ClientToBridgeWebappMsgDispatch for WebappHandler {
  type Output = HandlerResult;

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

  async fn activate(&self, params: WebappActivate) -> HandlerResult {
    let WebappActivate { id } = params;
    if self.handle.state.webapps.resolve(id).await.is_none() {
      self.handle.state.webapps.rescan().await;
    }
    if self.handle.state.webapps.resolve(id).await.is_none() {
      self
        .handle
        .respond_err::<WebappActivate>(WebappError::WebappNotFound { id: id.to_string() })
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

  async fn icon(&self, params: WebappIcon) -> HandlerResult {
    let WebappIcon { id } = params;
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
          .respond_err::<WebappIcon>(WebappError::IconNotAvailable { id: id.to_string() })
          .await?;
      }
    }
    Ok(())
  }

  async fn install_begin(&self, params: WebappInstallBegin) -> HandlerResult {
    let req = params;
    tracing::info!(
      "({:?}) WebappInstallBegin install_id={} sha256={} size={}",
      &self.handle.from,
      req.install_id,
      req.expected_sha256,
      req.expected_size,
    );
    match crate::install::install_begin(
      &self.handle.state,
      req.install_id,
      req.expected_sha256,
      req.expected_size,
    )
    .await
    {
      Ok(resume_from_offset) => {
        self
          .handle
          .respond_to::<WebappInstallBegin>(WebappInstallBeginAck { resume_from_offset })
          .await?
      }
      Err(err) => self.handle.respond_err::<WebappInstallBegin>(err).await?,
    }
    Ok(())
  }

  async fn install_chunk(&self, params: WebappInstallChunk) -> HandlerResult {
    let chunk = params;
    tracing::trace!(
      "({:?}) WebappInstallChunk install_id={} offset={} len={} last={}",
      &self.handle.from,
      chunk.install_id,
      chunk.offset,
      chunk.bytes.len(),
      chunk.last,
    );
    crate::install::accept_install_chunk(
      &self.handle.state,
      &self.handle.bluetooth,
      chunk.install_id,
      chunk.offset,
      chunk.bytes,
      chunk.last,
    )
    .await;
    Ok(())
  }

  async fn install_abandon(&self, params: WebappInstallAbandon) -> HandlerResult {
    let req = params;
    tracing::info!(
      "({:?}) WebappInstallAbandon install_id={}",
      &self.handle.from,
      req.install_id,
    );
    crate::install::install_abandon(&self.handle.state, req.install_id).await;
    Ok(())
  }
}
