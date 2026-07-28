//! Notifications surface - modeled on iAP2 Notification family / Apple
//! ANCS. Android's richer notification model maps down to this floor.
//! Two action slots (positive + negative), both optional; no reply text
//! input.

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
/// audibly" hint and `important` is the high-importance flag. Only
/// notifications posted while the daemon is connected are surfaced, so
/// there is no pre-existing/backfill marker.
#[typeshare]
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct NotificationFlags {
  pub silent: bool,
  pub important: bool,
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
/// - webapps pass it to `invokePositive`/`invokeNegative` and listen for
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

/// Daemon-observed state of the ANCS GATT-client session against the
/// connected iPhone. iOS-only; emitted on transitions so the companion
/// app can confirm the LE-pair + ANCS authorization handshake completed.
///
/// State machine:
/// - `Unknown`: pre-boot only (no iAP2 has ever attached this session).
/// - `Probing`: a session task is running but no determination yet,
///   OR iAP2 just detached and we expect to re-probe on reconnect.
/// - `Authorized`: ANCS attribute fetches are succeeding.
/// - `Unauthorized`: ANCS service hidden, or auth-gate detected.
#[typeshare]
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum AncsAuthState {
  #[default]
  Unknown,
  Probing,
  Authorized,
  Unauthorized,
}

/// Why a notification action slot could not be invoked. Both invoke verbs are
/// fire-and-forget commands, so a refusal has no reply to ride on.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum NotificationsError {
  /// The id is no longer in the companion's active notification set.
  NotFound { id: String },
  /// The slot is absent on this notification, or the platform refused it.
  ActionRejected { reason: String },
  /// No companion is connected, so there is nowhere to send the action.
  NoTarget,
}
