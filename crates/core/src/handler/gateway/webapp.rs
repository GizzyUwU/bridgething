use libbridgething::gateway::{
  GatewayToBridgeWebappMsg, GetActiveWebapp, ListWebapps, WebappActive, WebappError, WebappInstall, WebappList,
  WebappSwitchTo, WebappUninstall,
};

use crate::chrome::ChromeCommand;

use super::{HandlerResult, MsgHandle};

const KIOSK_HOME_URL: &str = "http://127.0.0.1:8891/";

#[derive(Debug)]
pub struct WebappHandler {
  handle: MsgHandle,
}

impl WebappHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(&mut self, msg: GatewayToBridgeWebappMsg) -> HandlerResult {
    tracing::debug!("({:?}) handling webapp message", &self.handle.address);

    match msg {
      GatewayToBridgeWebappMsg::List => self.list().await,
      GatewayToBridgeWebappMsg::GetActive => self.get_active().await,
      GatewayToBridgeWebappMsg::SwitchTo(req) => self.switch_to(req).await,
      GatewayToBridgeWebappMsg::Install(req) => self.install(req).await,
      GatewayToBridgeWebappMsg::Uninstall(req) => self.uninstall(req).await,
    }
  }

  async fn list(&self) -> HandlerResult {
    let webapps = self.handle.state.webapps.list().await;
    self.handle.respond_to::<ListWebapps>(WebappList { webapps }).await;
    Ok(())
  }

  async fn get_active(&self) -> HandlerResult {
    let name = self.handle.state.active_webapp().await;
    self.handle.respond_to::<GetActiveWebapp>(WebappActive { name }).await;
    Ok(())
  }

  async fn switch_to(&self, req: WebappSwitchTo) -> HandlerResult {
    let WebappSwitchTo { name } = req;
    if self.handle.state.webapps.resolve(&name).is_none() {
      tracing::warn!(
        "({:?}) refusing switch to unknown webapp {}",
        &self.handle.address,
        name
      );
      self
        .handle
        .respond_err::<WebappSwitchTo>(WebappError::UnknownWebapp { name })
        .await;
      return Ok(());
    }

    self.handle.state.set_active_webapp(name.clone()).await?;
    self.reload_kiosk().await;
    self.handle.respond_to::<WebappSwitchTo>(WebappActive { name }).await;
    Ok(())
  }

  async fn install(&self, req: WebappInstall) -> HandlerResult {
    let WebappInstall { name, archive } = req;
    let installed = self.handle.state.webapps.install(&name, archive).await?;
    self.handle.respond_to::<WebappInstall>(installed).await;
    Ok(())
  }

  async fn uninstall(&self, req: WebappUninstall) -> HandlerResult {
    let WebappUninstall { name } = req;
    if self.handle.state.webapps.is_builtin(&name) {
      tracing::warn!(
        "({:?}) refusing uninstall of built-in webapp {}",
        &self.handle.address,
        name
      );
      self
        .handle
        .respond_err::<WebappUninstall>(WebappError::CannotUninstallBuiltin { name })
        .await;
      return Ok(());
    }

    let removed = self.handle.state.webapps.uninstall(&name).await?;
    if !removed {
      tracing::debug!(
        "({:?}) webapp {} was not installed; nothing to do",
        &self.handle.address,
        name
      );
    }

    let mut active = self.handle.state.active_webapp().await;
    if active == name && self.handle.state.webapps.resolve(&active).is_none() {
      let fallback = "stock".to_string();
      tracing::info!("active webapp {} was uninstalled; falling back to {}", name, fallback);
      self.handle.state.set_active_webapp(fallback.clone()).await?;
      self.reload_kiosk().await;
      active = fallback;
    }

    self
      .handle
      .respond_to::<WebappUninstall>(WebappActive { name: active })
      .await;
    Ok(())
  }

  async fn reload_kiosk(&self) {
    if let Err(e) = self
      .handle
      .state
      .chrome
      .send(ChromeCommand::Navigate(KIOSK_HOME_URL.to_string()))
      .await
    {
      tracing::warn!("failed to reload kiosk after webapp switch: {:?}", e);
    }
  }
}
