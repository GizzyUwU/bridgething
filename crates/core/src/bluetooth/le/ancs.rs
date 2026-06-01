//! Apple Notification Center Service (ANCS) GATT client, as a consumer
//! of the shared LE session. Subscribes the Notification Source + Data
//! Source over the iPhone's LE-bonded GATT db, surfaces iOS
//! notifications to webapps, and forwards `notifications.invokePositive`
//! / `invokeNegative` back as PerformNotificationAction writes on the
//! Control Point.
//!
//! ## How iOS gates ANCS
//!
//! iOS exposes ANCS over an LE-bonded link only, and content access is
//! further gated on a per-bond authorization decision. The first ANCS
//! access that requires it pops the "Allow X to access notifications"
//! prompt; after accept, a "Share System Notifications" toggle appears
//! in the iPhone's Bluetooth Settings entry for the peer.
//!
//! Without authorization, the Notification Source CCCD subscribe still
//! works (it is a descriptor write, not gated on the decision), but
//! Control Point reads either error 0xA2 or never deliver on the Data
//! Source. The session surfaces that as consecutive attribute-fetch
//! timeouts and reports `Unauthorized`.

use std::{collections::VecDeque, pin::Pin, time::Duration};

use bluer::gatt::{
  WriteOp,
  remote::{Characteristic, CharacteristicWriteRequest, Service},
};
use futures::Stream;
use libbridgething::{
  AncsAuthState, DismissReason, Notification, NotificationApp, NotificationCategory, NotificationFlags,
  client::{BridgeToClientNotificationsMsgEvent, NotificationRemoved},
  gateway::BridgeToGatewayNotificationsMsgEvent,
};
use tokio::{sync::Mutex, time::Instant};
use uuid::Uuid;

use crate::{bluetooth::BluetoothMan, net::WireEventBus};

pub const ANCS_SERVICE: Uuid = Uuid::from_u128(0x7905F431_B5CE_4E99_A40F_4B1E122D00D0);
const NOTIFICATION_SOURCE: Uuid = Uuid::from_u128(0x9FBF120D_6301_42D9_8C58_25E699A21DBD);
const CONTROL_POINT: Uuid = Uuid::from_u128(0x69D1D8F3_45E1_49A8_9821_9BBDFDAAD9D9);
const DATA_SOURCE: Uuid = Uuid::from_u128(0x22EAC6E9_24D6_4BB5_BE44_B36ACE7C7BFB);

const COMMAND_GET_NOTIFICATION_ATTRIBUTES: u8 = 0x00;
const COMMAND_PERFORM_NOTIFICATION_ACTION: u8 = 0x02;

const ATTR_APP_IDENTIFIER: u8 = 0x00;
const ATTR_TITLE: u8 = 0x01;
const ATTR_SUBTITLE: u8 = 0x02;
const ATTR_MESSAGE: u8 = 0x03;
const ATTR_DATE: u8 = 0x05;
const ATTR_POSITIVE_ACTION_LABEL: u8 = 0x06;
const ATTR_NEGATIVE_ACTION_LABEL: u8 = 0x07;

const FLAG_SILENT: u8 = 0x01;
const FLAG_IMPORTANT: u8 = 0x02;
const FLAG_PRE_EXISTING: u8 = 0x04;
const FLAG_POSITIVE_ACTION: u8 = 0x08;
const FLAG_NEGATIVE_ACTION: u8 = 0x10;

const EVENT_ADDED: u8 = 0x00;
const EVENT_MODIFIED: u8 = 0x01;
const EVENT_REMOVED: u8 = 0x02;

pub const ACTION_POSITIVE: u8 = 0x00;
pub const ACTION_NEGATIVE: u8 = 0x01;

const TITLE_MAX: u16 = 256;
const SUBTITLE_MAX: u16 = 256;
const MESSAGE_MAX: u16 = 1024;

const PENDING_QUEUE_CAP: usize = 64;
const ATTRIBUTE_AUTH_PROBE_INTERVAL: Duration = Duration::from_secs(60);
const ATTRIBUTE_AUTH_GUIDANCE_THRESHOLD: u32 = 3;
pub const ATTRIBUTE_FETCH_TIMEOUT: Duration = Duration::from_secs(5);
pub const ANCS_ID_PREFIX: &str = "ancs:";

type NotifyStream = Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>;

#[derive(Clone)]
pub struct AuthStateReporter {
  bluetooth: BluetoothMan,
  state: std::sync::Arc<Mutex<AncsAuthState>>,
}

impl AuthStateReporter {
  pub fn new(bluetooth: BluetoothMan) -> Self {
    Self {
      bluetooth,
      state: std::sync::Arc::new(Mutex::new(AncsAuthState::Unknown)),
    }
  }

  pub async fn report(&self, next: AncsAuthState) {
    let mut guard = self.state.lock().await;
    if *guard == next {
      return;
    }
    *guard = next;
    drop(guard);
    self
      .bluetooth
      .gateway_man
      .broadcast(BridgeToGatewayNotificationsMsgEvent::AncsAuthStateChanged(next))
      .await;
  }
}

pub struct AncsStreams {
  pub notification_source: NotifyStream,
  pub data_source: NotifyStream,
}

pub struct Ancs {
  control_point: Characteristic,
  pending: VecDeque<PendingNotification>,
  in_flight: Option<PendingNotification>,
  ds_buffer: Vec<u8>,
  consecutive_timeouts: u32,
  last_auth_probe: Instant,
}

impl Ancs {
  pub async fn subscribe(service: &Service) -> AncsResult<(Self, AncsStreams)> {
    let chars = locate_characteristics(service).await?;
    let ns = chars.notification_source.notify().await?;
    let ds = chars.data_source.notify().await?;
    let me = Self {
      control_point: chars.control_point,
      pending: VecDeque::with_capacity(PENDING_QUEUE_CAP),
      in_flight: None,
      ds_buffer: Vec::with_capacity(2048),
      consecutive_timeouts: 0,
      last_auth_probe: Instant::now(),
    };
    let streams = AncsStreams {
      notification_source: Box::pin(ns),
      data_source: Box::pin(ds),
    };
    Ok((me, streams))
  }

  pub fn has_in_flight(&self) -> bool {
    self.in_flight.is_some()
  }

  pub fn pump_allowed(&self) -> bool {
    self.in_flight.is_none()
      && (self.consecutive_timeouts < ATTRIBUTE_AUTH_GUIDANCE_THRESHOLD
        || self.last_auth_probe.elapsed() >= ATTRIBUTE_AUTH_PROBE_INTERVAL)
  }

  pub async fn pump(&mut self) {
    let Some(item) = self.pending.pop_front() else {
      return;
    };
    let cmd = build_get_attributes(item.uid);
    if let Err(err) = self.control_point.write_ext(&cmd, &write_request()).await {
      tracing::warn!(uid = item.uid, ?err, "ANCS GNA write failed; dropping");
      return;
    }
    if self.consecutive_timeouts >= ATTRIBUTE_AUTH_GUIDANCE_THRESHOLD {
      self.last_auth_probe = Instant::now();
    }
    self.in_flight = Some(item);
  }

  pub async fn on_notification_source(&mut self, frame: &[u8], bus: &WireEventBus) {
    if frame.len() < 8 {
      tracing::trace!(len = frame.len(), "ANCS NS frame too short; dropping");
      return;
    }
    let event_id = frame[0];
    let flags = frame[1];
    let category = frame[2];
    let uid = u32::from_le_bytes([frame[4], frame[5], frame[6], frame[7]]);

    if event_id == EVENT_REMOVED {
      let event = BridgeToClientNotificationsMsgEvent::Removed(NotificationRemoved {
        id: format!("{ANCS_ID_PREFIX}{uid}"),
        reason: DismissReason::RemoteDismissed,
      });
      let _ = bus.broadcast_event(event).await;
      return;
    }

    if event_id != EVENT_ADDED && event_id != EVENT_MODIFIED {
      tracing::trace!(event_id, "ANCS NS frame with unknown EventID; dropping");
      return;
    }

    if self.pending.len() >= PENDING_QUEUE_CAP
      && let Some(d) = self.pending.pop_front()
    {
      tracing::warn!(dropped_uid = d.uid, "ANCS pending queue full; dropped oldest");
    }
    self.pending.push_back(PendingNotification {
      uid,
      event_id,
      flags,
      category,
    });
  }

  pub async fn on_data_source(&mut self, value: &[u8], bus: &WireEventBus) -> bool {
    self.ds_buffer.extend_from_slice(value);
    let mut drained = false;
    while let Some(meta) = self.in_flight.clone() {
      let Some((fields, consumed)) = parse_gna_response(&self.ds_buffer) else {
        break;
      };
      self.ds_buffer.drain(..consumed);
      self.in_flight = None;
      emit_notification(bus, &meta, fields).await;
      drained = true;
    }
    if drained {
      self.consecutive_timeouts = 0;
    }
    drained
  }

  pub async fn on_invoke(&self, uid: u32, action: u8) {
    if let Err(err) = write_perform_action(&self.control_point, uid, action).await {
      tracing::warn!(uid, action, ?err, "ANCS perform-action write failed");
    }
  }

  pub fn on_fetch_timeout(&mut self) -> bool {
    let Some(p) = self.in_flight.take() else {
      return false;
    };
    self.consecutive_timeouts = self.consecutive_timeouts.saturating_add(1);
    self.ds_buffer.clear();
    if self.consecutive_timeouts == ATTRIBUTE_AUTH_GUIDANCE_THRESHOLD {
      tracing::warn!(
        "ANCS notifications arriving but iOS is dropping or rejecting content reads"
      );
      true
    } else {
      tracing::debug!(
        uid = p.uid,
        consecutive_timeouts = self.consecutive_timeouts,
        "ANCS attribute fetch timed out"
      );
      false
    }
  }
}

fn write_request() -> CharacteristicWriteRequest {
  CharacteristicWriteRequest {
    op_type: WriteOp::Request,
    ..Default::default()
  }
}

struct AncsCharacteristics {
  notification_source: Characteristic,
  control_point: Characteristic,
  data_source: Characteristic,
}

async fn locate_characteristics(service: &Service) -> AncsResult<AncsCharacteristics> {
  let mut ns = None;
  let mut cp = None;
  let mut ds = None;
  for ch in service.characteristics().await? {
    match ch.uuid().await {
      Ok(u) if u == NOTIFICATION_SOURCE => ns = Some(ch),
      Ok(u) if u == CONTROL_POINT => cp = Some(ch),
      Ok(u) if u == DATA_SOURCE => ds = Some(ch),
      Ok(_) => {}
      Err(err) => tracing::trace!(?err, "ANCS characteristic UUID read failed"),
    }
  }
  Ok(AncsCharacteristics {
    notification_source: ns.ok_or(AncsError::CharacteristicMissing(NOTIFICATION_SOURCE))?,
    control_point: cp.ok_or(AncsError::CharacteristicMissing(CONTROL_POINT))?,
    data_source: ds.ok_or(AncsError::CharacteristicMissing(DATA_SOURCE))?,
  })
}

#[derive(Debug, Clone)]
struct PendingNotification {
  uid: u32,
  event_id: u8,
  flags: u8,
  category: u8,
}

fn build_get_attributes(uid: u32) -> Vec<u8> {
  let mut cmd = Vec::with_capacity(20);
  cmd.push(COMMAND_GET_NOTIFICATION_ATTRIBUTES);
  cmd.extend_from_slice(&uid.to_le_bytes());
  cmd.push(ATTR_APP_IDENTIFIER);
  cmd.push(ATTR_TITLE);
  cmd.extend_from_slice(&TITLE_MAX.to_le_bytes());
  cmd.push(ATTR_SUBTITLE);
  cmd.extend_from_slice(&SUBTITLE_MAX.to_le_bytes());
  cmd.push(ATTR_MESSAGE);
  cmd.extend_from_slice(&MESSAGE_MAX.to_le_bytes());
  cmd.push(ATTR_DATE);
  cmd.push(ATTR_POSITIVE_ACTION_LABEL);
  cmd.push(ATTR_NEGATIVE_ACTION_LABEL);
  cmd
}

async fn write_perform_action(control_point: &Characteristic, uid: u32, action: u8) -> AncsResult<()> {
  let mut cmd = Vec::with_capacity(6);
  cmd.push(COMMAND_PERFORM_NOTIFICATION_ACTION);
  cmd.extend_from_slice(&uid.to_le_bytes());
  cmd.push(action);
  control_point.write_ext(&cmd, &write_request()).await?;
  Ok(())
}

#[derive(Debug, Default)]
struct NotificationFields {
  app_id: Option<String>,
  title: Option<String>,
  subtitle: Option<String>,
  message: Option<String>,
  date_unix_s: Option<u32>,
  positive_label: Option<String>,
  negative_label: Option<String>,
}

fn parse_gna_response(buf: &[u8]) -> Option<(NotificationFields, usize)> {
  if buf.len() < 5 {
    return None;
  }
  if buf[0] != COMMAND_GET_NOTIFICATION_ATTRIBUTES {
    return None;
  }
  let mut idx = 5;
  let mut fields = NotificationFields::default();
  while idx < buf.len() {
    if idx + 3 > buf.len() {
      return None;
    }
    let attr_id = buf[idx];
    let len = u16::from_le_bytes([buf[idx + 1], buf[idx + 2]]) as usize;
    idx += 3;
    if idx + len > buf.len() {
      return None;
    }
    let value = &buf[idx..idx + len];
    let text = String::from_utf8_lossy(value).into_owned();
    match attr_id {
      ATTR_APP_IDENTIFIER => fields.app_id = Some(text),
      ATTR_TITLE => fields.title = Some(text),
      ATTR_SUBTITLE => fields.subtitle = Some(text),
      ATTR_MESSAGE => fields.message = Some(text),
      ATTR_DATE => fields.date_unix_s = parse_ancs_date(&text),
      ATTR_POSITIVE_ACTION_LABEL => fields.positive_label = Some(text),
      ATTR_NEGATIVE_ACTION_LABEL => fields.negative_label = Some(text),
      _ => {}
    }
    idx += len;
    if attr_id == ATTR_NEGATIVE_ACTION_LABEL || attr_id == ATTR_DATE {
      return Some((fields, idx));
    }
  }
  Some((fields, idx))
}

/// ANCS dates arrive as `yyyyMMdd'T'HHmmss` UTC.
fn parse_ancs_date(s: &str) -> Option<u32> {
  if s.len() != 15 || &s[8..9] != "T" {
    return None;
  }
  let year: i32 = s.get(0..4)?.parse().ok()?;
  let month: u32 = s.get(4..6)?.parse().ok()?;
  let day: u32 = s.get(6..8)?.parse().ok()?;
  let hour: u32 = s.get(9..11)?.parse().ok()?;
  let minute: u32 = s.get(11..13)?.parse().ok()?;
  let second: u32 = s.get(13..15)?.parse().ok()?;
  let days = days_from_civil(year, month, day)?;
  let secs = days * 86400 + hour as i64 * 3600 + minute as i64 * 60 + second as i64;
  u32::try_from(secs).ok()
}

fn days_from_civil(y: i32, m: u32, d: u32) -> Option<i64> {
  if !(1..=12).contains(&m) || d == 0 || d > 31 {
    return None;
  }
  let y = if m <= 2 { y - 1 } else { y };
  let era = y.div_euclid(400);
  let yoe = (y - era * 400) as i64;
  let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) as i64 + 2) / 5 + d as i64 - 1;
  let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
  Some(era as i64 * 146097 + doe - 719468)
}

async fn emit_notification(bus: &WireEventBus, meta: &PendingNotification, fields: NotificationFields) {
  let bundle_id = fields.app_id.unwrap_or_else(|| "unknown".to_string());
  let app = NotificationApp {
    bundle_id,
    display_name: None,
    icon_asset_id: None,
  };
  let category = decode_category(meta.category);
  let flags = NotificationFlags {
    silent: meta.flags & FLAG_SILENT != 0,
    important: meta.flags & FLAG_IMPORTANT != 0,
    pre_existing: meta.flags & FLAG_PRE_EXISTING != 0,
  };
  let positive_action = (meta.flags & FLAG_POSITIVE_ACTION != 0).then(|| libbridgething::NotificationAction {
    label: fields.positive_label.unwrap_or_else(|| "OK".to_string()),
  });
  let negative_action = (meta.flags & FLAG_NEGATIVE_ACTION != 0).then(|| libbridgething::NotificationAction {
    label: fields.negative_label.unwrap_or_else(|| "Dismiss".to_string()),
  });
  let notification = Notification {
    id: format!("{ANCS_ID_PREFIX}{}", meta.uid),
    app,
    category,
    title: fields.title,
    subtitle: fields.subtitle,
    message: fields.message,
    timestamp_unix_s: fields.date_unix_s,
    flags,
    positive_action,
    negative_action,
  };
  let event = match meta.event_id {
    EVENT_MODIFIED => BridgeToClientNotificationsMsgEvent::Updated(notification),
    _ => BridgeToClientNotificationsMsgEvent::Posted(notification),
  };
  let _ = bus.broadcast_event(event).await;
}

fn decode_category(byte: u8) -> NotificationCategory {
  match byte {
    1 => NotificationCategory::IncomingCall,
    2 => NotificationCategory::MissedCall,
    3 => NotificationCategory::Voicemail,
    4 => NotificationCategory::Social,
    5 => NotificationCategory::Schedule,
    6 => NotificationCategory::Email,
    7 => NotificationCategory::News,
    8 => NotificationCategory::HealthAndFitness,
    9 => NotificationCategory::BusinessAndFinance,
    10 => NotificationCategory::Location,
    11 => NotificationCategory::Entertainment,
    _ => NotificationCategory::Other,
  }
}

pub type AncsResult<T> = Result<T, AncsError>;

#[derive(Debug, thiserror::Error)]
pub enum AncsError {
  #[error("ANCS characteristic {0} not found")]
  CharacteristicMissing(Uuid),
  #[error(transparent)]
  Bluer(#[from] bluer::Error),
}
