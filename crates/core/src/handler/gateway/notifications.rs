use libbridgething::{
  Notification,
  client::{BridgeToClientNotificationsMsgEvent, NotificationRemoved as ClientNotificationRemoved},
  gateway::{GatewayToBridgeNotificationsMsgEventDispatch, NotificationRemoved},
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

impl GatewayToBridgeNotificationsMsgEventDispatch for NotificationsHandler {
  type Output = HandlerResult;

  async fn posted(&self, params: Notification) -> HandlerResult {
    self
      .handle
      .state
      .bus
      .broadcast_event(BridgeToClientNotificationsMsgEvent::Posted(params))
      .await?;
    Ok(())
  }

  async fn updated(&self, params: Notification) -> HandlerResult {
    self
      .handle
      .state
      .bus
      .broadcast_event(BridgeToClientNotificationsMsgEvent::Updated(params))
      .await?;
    Ok(())
  }

  async fn removed(&self, params: NotificationRemoved) -> HandlerResult {
    self
      .handle
      .state
      .bus
      .broadcast_event(BridgeToClientNotificationsMsgEvent::Removed(
        ClientNotificationRemoved {
          id: params.id,
          reason: params.reason,
        },
      ))
      .await?;
    Ok(())
  }
}
