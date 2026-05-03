use libbridgething::client::ClientToBridgeNotificationsMsg;

use super::{HandlerResult, MsgHandle};

pub struct NotificationsHandler {
  handle: MsgHandle,
}

impl NotificationsHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: ClientToBridgeNotificationsMsg) -> HandlerResult {
    match msg {
      ClientToBridgeNotificationsMsg::List(_) => Ok(self.handle.unimplemented("notifications.list").await?),
      ClientToBridgeNotificationsMsg::InvokePositive(_) => {
        Ok(self.handle.unimplemented("notifications.invokePositive").await?)
      }
      ClientToBridgeNotificationsMsg::InvokeNegative(_) => {
        Ok(self.handle.unimplemented("notifications.invokeNegative").await?)
      }
    }
  }
}
