use libbridgething::{
  NotificationsError,
  client::{
    BridgeToClientNotificationsMsgEvent, ClientToBridgeNotificationsMsgDispatch,
    NotificationInvoke as ClientNotificationInvoke, NotificationsErrorReply,
  },
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

  async fn has_gateway(&self) -> bool {
    if self.handle.state.capabilities.snapshot().gateway.is_some() {
      return true;
    }
    let event = BridgeToClientNotificationsMsgEvent::ErrorEvent(NotificationsErrorReply {
      error: NotificationsError::NoTarget,
    });
    if let Err(err) = self.handle.state.bus.send_event(self.handle.from, event).await {
      tracing::warn!(?err, "failed to report notification action failure to webapp");
    }
    false
  }
}

impl ClientToBridgeNotificationsMsgDispatch for NotificationsHandler {
  type Output = HandlerResult;

  async fn invoke_positive(&self, params: ClientNotificationInvoke) -> HandlerResult {
    if self.handle.bluetooth.le.try_invoke_positive(&params.id).await {
      return Ok(());
    }
    if !self.has_gateway().await {
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
    if self.handle.bluetooth.le.try_invoke_negative(&params.id).await {
      return Ok(());
    }
    if !self.has_gateway().await {
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
