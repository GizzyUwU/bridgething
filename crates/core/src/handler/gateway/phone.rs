use libbridgething::gateway::GatewayToBridgePhoneMsg;

use super::{HandlerResult, MsgHandle};

pub struct PhoneHandler {
  handle: MsgHandle,
}

impl PhoneHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: GatewayToBridgePhoneMsg) -> HandlerResult {
    match msg {
      GatewayToBridgePhoneMsg::Snapshot(_) => self.handle.unimplemented("gateway:phone.snapshot").await,
      GatewayToBridgePhoneMsg::CommunicationsSnapshot(_) => {
        self.handle.unimplemented("gateway:phone.communicationsSnapshot").await
      }
      GatewayToBridgePhoneMsg::CallStarted(_) => self.handle.unimplemented("gateway:phone.callStarted").await,
      GatewayToBridgePhoneMsg::CallUpdated(_) => self.handle.unimplemented("gateway:phone.callUpdated").await,
      GatewayToBridgePhoneMsg::CallEnded(_) => self.handle.unimplemented("gateway:phone.callEnded").await,
      GatewayToBridgePhoneMsg::StateReply(_) => self.handle.unimplemented("gateway:phone.stateReply").await,
    }
    Ok(())
  }
}
