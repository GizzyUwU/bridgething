use std::sync::Arc;

use bridgething_gateway::{OutboundLink, OutboundLinkExt};
use libbridgething::{
  DismissReason as WireDismissReason, Notification, NotificationAction as WireAction, NotificationApp as WireApp,
  NotificationCategory as WireCategory, NotificationFlags as WireFlags, NotificationsError,
  gateway::{
    GatewayToBridgeNotificationsMsgEvent, NotificationInvoke, NotificationRemoved as WireRemoved,
    NotificationsErrorReply,
  },
  wire::WireError,
};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::{
  backend::{
    ActionSink, DismissReason, NotificationAction, NotificationActionError, NotificationApp, NotificationBackend,
    NotificationCategory, NotificationEvent, NotificationInbox, NotificationRemoved, WireNotification,
  },
  dispatch::{Relay, Serial, tell},
};

enum Slot {
  Positive,
  Negative,
}

pub struct NotificationDispatcher {
  backend: Arc<dyn NotificationBackend>,
  link: Arc<dyn OutboundLink>,
  enabled: Arc<dyn Fn() -> bool + Send + Sync>,
  invokes: Serial,
  relay: Relay,
}

impl NotificationDispatcher {
  pub fn new(
    backend: Arc<dyn NotificationBackend>,
    link: Arc<dyn OutboundLink>,
    enabled: Arc<dyn Fn() -> bool + Send + Sync>,
  ) -> Self {
    Self {
      backend,
      link,
      enabled,
      invokes: Serial::spawn(),
      relay: Relay::default(),
    }
  }

  pub async fn start(&self) {
    let (inbox, events) = NotificationInbox::channel();
    self
      .relay
      .hold(tokio::spawn(relay(events, self.link.clone(), self.enabled.clone())));
    tell(&self.backend, move |backend| backend.start(inbox)).await;
  }

  pub async fn stop(&self) {
    self.relay.release();
    tell(&self.backend, |backend| backend.stop()).await;
  }

  pub async fn invoke_positive(&self, payload: NotificationInvoke) -> Result<(), WireError> {
    self.queue(Slot::Positive, payload.id);
    Ok(())
  }

  pub async fn invoke_negative(&self, payload: NotificationInvoke) -> Result<(), WireError> {
    self.queue(Slot::Negative, payload.id);
    Ok(())
  }

  fn queue(&self, slot: Slot, id: String) {
    self
      .invokes
      .push(invoke(self.backend.clone(), self.link.clone(), slot, id));
  }
}

async fn invoke(backend: Arc<dyn NotificationBackend>, link: Arc<dyn OutboundLink>, slot: Slot, id: String) {
  let (sink, answer) = ActionSink::channel();
  tell(&backend, move |backend| match slot {
    Slot::Positive => backend.invoke_positive(id, sink),
    Slot::Negative => backend.invoke_negative(id, sink),
  })
  .await;

  let refusal = match answer.await {
    Ok(None) => return,
    Ok(Some(error)) => wire_action_error(error),
    Err(_) => NotificationsError::ActionRejected {
      reason: "the notification action was never answered".into(),
    },
  };
  let _ = link
    .event(GatewayToBridgeNotificationsMsgEvent::ErrorEvent(
      NotificationsErrorReply { error: refusal },
    ))
    .await;
}

async fn relay(
  mut events: UnboundedReceiver<NotificationEvent>,
  link: Arc<dyn OutboundLink>,
  enabled: Arc<dyn Fn() -> bool + Send + Sync>,
) {
  while let Some(event) = events.recv().await {
    if !enabled() {
      continue;
    }
    let _ = match event {
      NotificationEvent::Posted(notification) => {
        link
          .event(GatewayToBridgeNotificationsMsgEvent::Posted(wire_notification(
            notification,
          )))
          .await
      }
      NotificationEvent::Removed(removed) => {
        link
          .event(GatewayToBridgeNotificationsMsgEvent::Removed(wire_removed(removed)))
          .await
      }
    };
  }
}

fn wire_notification(notification: WireNotification) -> Notification {
  Notification {
    id: notification.id,
    app: wire_app(notification.app),
    category: wire_category(notification.category),
    title: notification.title,
    subtitle: notification.subtitle,
    message: notification.message,
    timestamp_unix_s: notification.timestamp_unix_s,
    flags: WireFlags {
      silent: notification.flags.silent,
      important: notification.flags.important,
    },
    positive_action: notification.positive_action.map(wire_action),
    negative_action: notification.negative_action.map(wire_action),
  }
}

fn wire_app(app: NotificationApp) -> WireApp {
  WireApp {
    bundle_id: app.bundle_id,
    display_name: app.display_name,
    icon_asset_id: app.icon_asset_id,
  }
}

fn wire_action(action: NotificationAction) -> WireAction {
  WireAction { label: action.label }
}

fn wire_category(category: NotificationCategory) -> WireCategory {
  match category {
    NotificationCategory::Other => WireCategory::Other,
    NotificationCategory::IncomingCall => WireCategory::IncomingCall,
    NotificationCategory::MissedCall => WireCategory::MissedCall,
    NotificationCategory::Voicemail => WireCategory::Voicemail,
    NotificationCategory::Social => WireCategory::Social,
    NotificationCategory::Schedule => WireCategory::Schedule,
    NotificationCategory::Email => WireCategory::Email,
    NotificationCategory::News => WireCategory::News,
    NotificationCategory::HealthAndFitness => WireCategory::HealthAndFitness,
    NotificationCategory::BusinessAndFinance => WireCategory::BusinessAndFinance,
    NotificationCategory::Location => WireCategory::Location,
    NotificationCategory::Entertainment => WireCategory::Entertainment,
  }
}

fn wire_removed(removed: NotificationRemoved) -> WireRemoved {
  WireRemoved {
    id: removed.id,
    reason: match removed.reason {
      DismissReason::UserDismissed => WireDismissReason::UserDismissed,
      DismissReason::Acted => WireDismissReason::Acted,
      DismissReason::RemoteDismissed => WireDismissReason::RemoteDismissed,
    },
  }
}

fn wire_action_error(error: NotificationActionError) -> NotificationsError {
  match error {
    NotificationActionError::NotFound { id } => NotificationsError::NotFound { id },
    NotificationActionError::ActionRejected { reason } => NotificationsError::ActionRejected { reason },
    NotificationActionError::NoTarget => NotificationsError::NoTarget,
  }
}
