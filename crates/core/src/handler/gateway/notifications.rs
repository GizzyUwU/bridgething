use libbridgething::{
  Notification,
  client::{
    BridgeToClientNotificationsMsgEvent, NotificationRemoved as ClientNotificationRemoved,
    NotificationsErrorReply as ClientNotificationsErrorReply,
  },
  gateway::{GatewayToBridgeNotificationsMsgEventDispatch, NotificationRemoved, NotificationsErrorReply},
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

  async fn error_event(&self, params: NotificationsErrorReply) -> HandlerResult {
    tracing::warn!(error = ?params.error, "companion refused a notification action");
    self
      .handle
      .state
      .bus
      .broadcast_event(BridgeToClientNotificationsMsgEvent::ErrorEvent(
        ClientNotificationsErrorReply { error: params.error },
      ))
      .await?;
    Ok(())
  }
}
