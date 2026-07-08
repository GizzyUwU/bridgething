use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{DismissReason, Notification};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct NotificationRemoved {
  /// Id of the notification that was removed, from `Notification.id`.
  pub id: String,
  pub reason: DismissReason,
}

/// Daemon -> webapp notification mirror. `Posted` fires for a new
/// notification, `Updated` when an existing one's content changes, and
/// `Removed` when it is dismissed, acted on, or cleared remotely.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::BridgeToClientMsgData)]
pub enum BridgeToClientNotificationsMsg {
  #[bridge_event]
  Posted(Notification),
  #[bridge_event]
  Updated(Notification),
  #[bridge_event]
  Removed(NotificationRemoved),
}
