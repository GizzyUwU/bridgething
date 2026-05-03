//! Notifications surface — modeled on iAP2 Notification family / Apple
//! ANCS. Android's richer notification model maps down to this floor.
//! Two action slots (positive + negative), both optional; reply text
//! input is out of scope for v1 (Phone surface owns that).

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

#[typeshare]
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum NotificationCategory {
  #[default]
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

/// Originating app metadata. `bundle_id` is platform-stable
/// (`com.apple.MobileSMS`, `com.spotify.client`, etc.); `display_name`
/// and `icon_asset_id` are best-effort and may be missing on Android
/// gateways that don't surface them cheaply.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct NotificationApp {
  pub bundle_id: String,
  pub display_name: Option<String>,
  pub icon_asset_id: Option<String>,
}

/// ANCS-shaped flags. `silent` mirrors the iOS "do not surface
/// audibly" hint, `important` is the high-importance flag, and
/// `pre_existing` is true for notifications that arrived before the
/// daemon connected (replayed by the companion on first sync).
#[typeshare]
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct NotificationFlags {
  pub silent: bool,
  pub important: bool,
  pub pre_existing: bool,
}

/// One ANCS-style action slot. `label` is the gateway-localized prompt
/// the webapp renders on the action button.
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct NotificationAction {
  pub label: String,
}

/// One notification surfaced from the connected companion's notification
/// center. `id` is companion-stable for the lifetime of the notification
/// — webapps pass it to `invokePositive`/`invokeNegative` and listen for
/// `onNotificationRemoved`. Bodies (`title`/`subtitle`/`message`) are all
/// optional because ANCS treats them as separate attribute fetches.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct Notification {
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

/// Why a notification went away. `Acted` covers both positive and
/// negative invokes; gateways that distinguish dismiss-vs-acted may
/// surface both as `Acted`.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum DismissReason {
  UserDismissed,
  Acted,
  RemoteDismissed,
}

/// Page of notifications returned from `notifications.list`. Gateways
/// page large notification centers; webapps drive pagination via
/// `next_page_token`.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct NotificationsPage {
  pub items: Vec<Notification>,
  pub next_page_token: Option<String>,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum NotificationError {
  /// The named notification id does not exist (likely already dismissed).
  NotFound { id: String },
  /// The notification has no action slot in the requested polarity.
  NoActionAvailable,
  /// The companion or platform refused the action.
  ActionRejected { reason: String },
}
