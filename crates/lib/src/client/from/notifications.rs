use bridgething_macros::{BridgeEnum, WireRequest};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[wire_request(
  direction = ClientToBridge,
  surface = Notifications,
  request_variant = List,
  response = crate::client::NotificationsListReply,
  response_variant = ListReply,
  error = crate::client::NotificationsErrorReply,
  error_variant = ErrorReply,
)]
pub struct NotificationsList {
  pub page_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct NotificationInvoke {
  pub id: String,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::ClientToBridgeMsgData)]
pub enum ClientToBridgeNotificationsMsg {
  #[bridge_request]
  List(NotificationsList),
  #[bridge_command]
  InvokePositive(NotificationInvoke),
  #[bridge_command]
  InvokeNegative(NotificationInvoke),
}
