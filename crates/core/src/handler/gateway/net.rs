use libbridgething::gateway::GatewayToBridgeNetMsg;

use super::{HandlerResult, MsgHandle};

pub struct NetHandler {
  handle: MsgHandle,
}

impl NetHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: GatewayToBridgeNetMsg) -> HandlerResult {
    match msg {
      GatewayToBridgeNetMsg::FetchReply(_) => self.handle.unimplemented("gateway:net.fetchReply").await,
      GatewayToBridgeNetMsg::FetchErrorReply(_) => self.handle.unimplemented("gateway:net.fetchErrorReply").await,
      GatewayToBridgeNetMsg::FetchStreamBegin(_) => self.handle.unimplemented("gateway:net.fetchStreamBegin").await,
      GatewayToBridgeNetMsg::FetchStreamChunk(_) => self.handle.unimplemented("gateway:net.fetchStreamChunk").await,
      GatewayToBridgeNetMsg::FetchStreamEnd(_) => self.handle.unimplemented("gateway:net.fetchStreamEnd").await,
      GatewayToBridgeNetMsg::WsOpenReply(_) => self.handle.unimplemented("gateway:net.wsOpenReply").await,
      GatewayToBridgeNetMsg::WsErrorReply(_) => self.handle.unimplemented("gateway:net.wsErrorReply").await,
      GatewayToBridgeNetMsg::WsOpened(_) => self.handle.unimplemented("gateway:net.wsOpened").await,
      GatewayToBridgeNetMsg::WsMessage(_) => self.handle.unimplemented("gateway:net.wsMessage").await,
      GatewayToBridgeNetMsg::WsClosed(_) => self.handle.unimplemented("gateway:net.wsClosed").await,
      GatewayToBridgeNetMsg::WsErrorEvent(_) => self.handle.unimplemented("gateway:net.wsErrorEvent").await,
    }
    Ok(())
  }
}
