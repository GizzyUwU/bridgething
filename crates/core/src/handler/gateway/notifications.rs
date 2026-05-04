use libbridgething::{
  client::{BridgeToClientNotificationsMsgEvent, NotificationRemoved as ClientNotificationRemoved},
  gateway::GatewayToBridgeNotificationsMsgEvent,
};

use super::{HandlerResult, MsgHandle};

pub struct NotificationsHandler {
  handle: MsgHandle,
}

impl NotificationsHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: GatewayToBridgeNotificationsMsgEvent) -> HandlerResult {
    let event = match msg {
      GatewayToBridgeNotificationsMsgEvent::Posted(notification) => {
        BridgeToClientNotificationsMsgEvent::Posted(notification)
      }
      GatewayToBridgeNotificationsMsgEvent::Updated(notification) => {
        BridgeToClientNotificationsMsgEvent::Updated(notification)
      }
      GatewayToBridgeNotificationsMsgEvent::Removed(removed) => {
        BridgeToClientNotificationsMsgEvent::Removed(ClientNotificationRemoved {
          id: removed.id,
          reason: removed.reason,
        })
      }
    };
    self.handle.state.bus.broadcast_event(event).await?;
    Ok(())
  }
}
