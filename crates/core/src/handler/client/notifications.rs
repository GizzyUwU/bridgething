use libbridgething::{
  client::{ClientToBridgeNotificationsMsg, NotificationInvoke as ClientNotificationInvoke},
  gateway::{self, BridgeToGatewayNotificationsMsgCommand},
};

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
      ClientToBridgeNotificationsMsg::InvokePositive(invoke) => {
        if self.handle.state.ancs.try_invoke_positive(&invoke.id).await {
          return Ok(());
        }
        self
          .handle
          .bluetooth
          .gateway_man
          .broadcast_command(BridgeToGatewayNotificationsMsgCommand::InvokePositive(to_gateway(
            invoke,
          )))
          .await;
      }
      ClientToBridgeNotificationsMsg::InvokeNegative(invoke) => {
        if self.handle.state.ancs.try_invoke_negative(&invoke.id).await {
          return Ok(());
        }
        self
          .handle
          .bluetooth
          .gateway_man
          .broadcast_command(BridgeToGatewayNotificationsMsgCommand::InvokeNegative(to_gateway(
            invoke,
          )))
          .await;
      }
    }
    Ok(())
  }
}

fn to_gateway(invoke: ClientNotificationInvoke) -> gateway::NotificationInvoke {
  gateway::NotificationInvoke { id: invoke.id }
}
