use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NotificationCategory {
  Other,
  IncomingCall,
  MissedCall,
  Voicemail,
  Social,
  Schedule,
  Email,
  News,
  HealthAndFitness,
  BusinessAndFinance,
  Location,
  Entertainment,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NotificationApp {
  pub bundle_id: String,
  pub display_name: Option<String>,
  pub icon_asset_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct NotificationFlags {
  pub silent: bool,
  pub important: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NotificationAction {
  pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WireNotification {
  pub id: String,
  pub app: NotificationApp,
  pub category: NotificationCategory,
  pub title: Option<String>,
  pub subtitle: Option<String>,
  pub message: Option<String>,
  pub timestamp_unix_s: Option<u32>,
  pub flags: NotificationFlags,
  pub positive_action: Option<NotificationAction>,
  pub negative_action: Option<NotificationAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum DismissReason {
  UserDismissed,
  Acted,
  RemoteDismissed,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NotificationRemoved {
  pub id: String,
  pub reason: DismissReason,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum NotificationActionError {
  NotFound { id: String },
  ActionRejected { reason: String },
  NoTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationEvent {
  Posted(WireNotification),
  Removed(NotificationRemoved),
}

#[uniffi::export(with_foreign)]
pub trait NotificationBackend: Send + Sync {
  fn start(&self, inbox: Arc<NotificationInbox>);
  fn stop(&self);
  fn invoke_positive(&self, id: String, sink: Arc<ActionSink>);
  fn invoke_negative(&self, id: String, sink: Arc<ActionSink>);
}

#[derive(uniffi::Object)]
pub struct NotificationInbox {
  tx: mpsc::UnboundedSender<NotificationEvent>,
}

impl NotificationInbox {
  pub fn channel() -> (Arc<Self>, mpsc::UnboundedReceiver<NotificationEvent>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (Arc::new(Self { tx }), rx)
  }
}

#[uniffi::export]
impl NotificationInbox {
  pub fn on_posted(&self, notification: WireNotification) {
    let _ = self.tx.send(NotificationEvent::Posted(notification));
  }

  pub fn on_removed(&self, removed: NotificationRemoved) {
    let _ = self.tx.send(NotificationEvent::Removed(removed));
  }
}

#[derive(uniffi::Object)]
pub struct ActionSink {
  tx: std::sync::Mutex<Option<oneshot::Sender<Option<NotificationActionError>>>>,
}

impl ActionSink {
  pub fn channel() -> (Arc<Self>, oneshot::Receiver<Option<NotificationActionError>>) {
    let (tx, rx) = oneshot::channel();
    (
      Arc::new(Self {
        tx: std::sync::Mutex::new(Some(tx)),
      }),
      rx,
    )
  }

  fn settle(&self, outcome: Option<NotificationActionError>) {
    if let Some(tx) = self.tx.lock().unwrap().take() {
      let _ = tx.send(outcome);
    }
  }
}

#[uniffi::export]
impl ActionSink {
  pub fn complete(&self) {
    self.settle(None);
  }

  pub fn fail(&self, error: NotificationActionError) {
    self.settle(Some(error));
  }
}
