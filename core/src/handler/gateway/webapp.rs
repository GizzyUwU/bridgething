use libbridgething::gateway::{
  BridgeToGatewayMsgData, BridgeToGatewayWebappMsg, GatewayToBridgeWebappMsg, WebappActive, WebappInstall, WebappList,
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
      GatewayToBridgeWebappMsg::SwitchTo(WebappSwitchTo { name }) => self.switch_to(name).await,
      GatewayToBridgeWebappMsg::Install(WebappInstall { name, archive }) => self.install(name, archive).await,
      GatewayToBridgeWebappMsg::Uninstall(WebappUninstall { name }) => self.uninstall(name).await,
    }
  }

  async fn list(&self) -> HandlerResult {
    let webapps = self.handle.state.webapps.list().await;
    self
      .handle
      .respond(BridgeToGatewayWebappMsg::Webapps(WebappList { webapps }))
      .await;
    Ok(())
  }

  async fn get_active(&self) -> HandlerResult {
    let name = self.handle.state.active_webapp().await;
    self
      .handle
      .respond(BridgeToGatewayWebappMsg::Active(WebappActive { name }))
      .await;
    Ok(())
  }

  async fn switch_to(&self, name: String) -> HandlerResult {
    if self.handle.state.webapps.resolve(&name).is_none() {
      tracing::warn!(
        "({:?}) refusing switch to unknown webapp {}",
        &self.handle.address,
        name
      );
      self.handle.respond(BridgeToGatewayMsgData::Nack).await;
      return Ok(());
    }

    self.handle.state.set_active_webapp(name.clone()).await?;
    self.reload_kiosk().await;
    self
      .handle
      .respond(BridgeToGatewayWebappMsg::Switched(WebappActive { name }))
      .await;
    Ok(())
  }

  async fn install(&self, name: String, archive: Vec<u8>) -> HandlerResult {
    let installed = self.handle.state.webapps.install(&name, archive).await?;
    self
      .handle
      .respond(BridgeToGatewayWebappMsg::Webapps(WebappList {
        webapps: vec![installed],
      }))
      .await;
    Ok(())
  }

  async fn uninstall(&self, name: String) -> HandlerResult {
    if self.handle.state.webapps.is_builtin(&name) {
      tracing::warn!(
        "({:?}) refusing uninstall of built-in webapp {}",
        &self.handle.address,
        name
      );
      self.handle.respond(BridgeToGatewayMsgData::Nack).await;
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

    let active = self.handle.state.active_webapp().await;
    if active == name && self.handle.state.webapps.resolve(&active).is_none() {
      let fallback = "stock".to_string();
      tracing::info!("active webapp {} was uninstalled; falling back to {}", name, fallback);
      self.handle.state.set_active_webapp(fallback).await?;
      self.reload_kiosk().await;
    }

    self.handle.respond(BridgeToGatewayMsgData::Done).await;
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
