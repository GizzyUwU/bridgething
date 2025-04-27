use libbridgething::gateway::{BridgeToGatewayMsgData, GatewayToBridgeChromeMsg};

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
      GatewayToBridgeChromeMsg::Navigate { url } => self.navigate(url).await,
    }
  }

  pub async fn navigate(&self, url: String) -> HandlerResult {
    tracing::debug!("({:?}) navigating to {:?}", &self.handle.address, url);

    self.handle.state.chrome.send(ChromeCommand::Navigate(url)).await;
    self.handle.respond(BridgeToGatewayMsgData::Ack).await;

    Ok(())
  }
}
