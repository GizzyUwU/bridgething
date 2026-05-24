use libbridgething::{
  client::{ClientToBridgeNotificationsMsgDispatch, NotificationInvoke as ClientNotificationInvoke},
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
}

impl ClientToBridgeNotificationsMsgDispatch for NotificationsHandler {
  type Output = HandlerResult;

  async fn invoke_positive(&self, params: ClientNotificationInvoke) -> HandlerResult {
    if self.handle.state.ancs.try_invoke_positive(&params.id).await {
      return Ok(());
    }
    self
      .handle
      .bluetooth
      .gateway_man
      .broadcast_command(BridgeToGatewayNotificationsMsgCommand::InvokePositive(to_gateway(
        params,
      )))
      .await;
    Ok(())
  }

  async fn invoke_negative(&self, params: ClientNotificationInvoke) -> HandlerResult {
    if self.handle.state.ancs.try_invoke_negative(&params.id).await {
      return Ok(());
    }
    self
      .handle
      .bluetooth
      .gateway_man
      .broadcast_command(BridgeToGatewayNotificationsMsgCommand::InvokeNegative(to_gateway(
        params,
      )))
      .await;
    Ok(())
  }
}

fn to_gateway(invoke: ClientNotificationInvoke) -> gateway::NotificationInvoke {
  gateway::NotificationInvoke { id: invoke.id }
}
