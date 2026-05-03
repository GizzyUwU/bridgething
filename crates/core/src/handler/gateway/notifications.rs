use libbridgething::gateway::GatewayToBridgeNotificationsMsg;

use super::{HandlerResult, MsgHandle};

pub struct NotificationsHandler {
  handle: MsgHandle,
}

impl NotificationsHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: GatewayToBridgeNotificationsMsg) -> HandlerResult {
    match msg {
      GatewayToBridgeNotificationsMsg::ListReply(_) => {
        self.handle.unimplemented("gateway:notifications.listReply").await
      }
      GatewayToBridgeNotificationsMsg::ErrorReply(_) => {
        self.handle.unimplemented("gateway:notifications.errorReply").await
      }
      GatewayToBridgeNotificationsMsg::Posted(_) => self.handle.unimplemented("gateway:notifications.posted").await,
      GatewayToBridgeNotificationsMsg::Updated(_) => self.handle.unimplemented("gateway:notifications.updated").await,
      GatewayToBridgeNotificationsMsg::Removed(_) => self.handle.unimplemented("gateway:notifications.removed").await,
    }
    Ok(())
  }
}
