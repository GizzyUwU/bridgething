use libbridgething::gateway::{BridgeToGatewayMsgData, ChromeNavigate, GatewayToBridgeChromeMsg};

use crate::chrome::ChromeCommand;

use super::{HandlerResult, MsgHandle};

#[derive(Debug)]
pub struct ChromeHandler {
  handle: MsgHandle,
}

impl ChromeHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(&mut self, msg: GatewayToBridgeChromeMsg) -> HandlerResult {
    tracing::debug!("({:?}) handling chrome message", &self.handle.address);

    match msg {
      GatewayToBridgeChromeMsg::Navigate(ChromeNavigate { url }) => self.navigate(url).await,
    }
  }

  pub async fn navigate(&self, url: String) -> HandlerResult {
    tracing::debug!("({:?}) navigating to {:?}", &self.handle.address, url);

    if let Err(err) = self.handle.state.chrome.send(ChromeCommand::Navigate(url)).await {
      tracing::error!(
        "({:?}) error sending command to chrome: {:?}",
        &self.handle.address,
        err
      );
    };
    self.handle.respond(BridgeToGatewayMsgData::Ack).await;

    Ok(())
  }
}
