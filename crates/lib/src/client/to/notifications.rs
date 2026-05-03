use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{DismissReason, Notification, NotificationError, NotificationsPage};

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct NotificationsListReply {
  pub page: NotificationsPage,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct NotificationsErrorReply {
  pub error: NotificationError,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct NotificationRemoved {
  pub id: String,
  pub reason: DismissReason,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::BridgeToClientMsgData)]
pub enum BridgeToClientNotificationsMsg {
  #[bridge_response]
  ListReply(NotificationsListReply),
  #[bridge_response]
  ErrorReply(NotificationsErrorReply),
  #[bridge_event]
  Posted(Notification),
  #[bridge_event]
  Updated(Notification),
  #[bridge_event]
  Removed(NotificationRemoved),
}
