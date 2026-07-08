use bridgething_macros::{BridgeDispatch, BridgeEnum};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Payload for `invokePositive` and `invokeNegative`.
pub struct NotificationInvoke {
  /// Notification id to act on, from `Notification.id`.
  pub id: String,
}

/// Webapp -> daemon notification action surface. Fire-and-forget; invokes
/// the notification's positive or negative ANCS-style action slot on the
/// connected companion. Prefer `positiveAction`/`negativeAction` on the
/// `Notification` to decide whether a slot exists before invoking it.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum, BridgeDispatch)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::ClientToBridgeMsgData)]
pub enum ClientToBridgeNotificationsMsg {
  #[bridge_command]
  InvokePositive(NotificationInvoke),
  #[bridge_command]
  InvokeNegative(NotificationInvoke),
}
