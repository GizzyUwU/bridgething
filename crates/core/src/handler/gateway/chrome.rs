use libbridgething::gateway::{BridgeToGatewayMsgData, ChromeNavigate, GatewayToBridgeChromeMsgCommandDispatch};

use super::{HandlerResult, MsgHandle};
use crate::chrome::ChromeCommand;

#[derive(Debug)]
pub struct ChromeHandler {
  handle: MsgHandle,
}

impl ChromeHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }
}

impl GatewayToBridgeChromeMsgCommandDispatch for ChromeHandler {
  type Output = HandlerResult;

  async fn navigate(&self, params: ChromeNavigate) -> HandlerResult {
    let ChromeNavigate { url } = params;
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
