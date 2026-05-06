//! Apple Notification Center Service (ANCS) GATT client. Subscribes
//! over the same paired BLE connection BlueZ already maintains for an
//! iPhone, surfaces iOS notifications to webapps, and forwards
//! `notifications.invokePositive` / `invokeNegative` back as
//! PerformNotificationAction writes on the Control Point.
//!
//! Lifecycle is tied to iAP2 link state. `attach(addr)` kicks off after
//! `Iap2EventRouter` sees `LinkEstablished` (iPhone is paired and
//! connected at the BlueZ level by then, so btleplug enumerates it
//! immediately). `detach(addr)` runs on `LinkDown`. On disconnect /
//! GATT error the inner task exits and waits for the next attach.
//!
//! Wire mapping: every ANCS notification surfaces with id
//! `"ancs:<NotificationUID>"`. The notifications handler peels that
//! prefix to choose between writing a CP command (here) versus
//! broadcasting an `InvokePositive/Negative` to the companion gateway.

use std::{collections::VecDeque, sync::Arc, time::Duration};

use bluer::Address;
use btleplug::{
  api::{Central, CentralEvent, Characteristic, Manager as _, Peripheral as _, WriteType},
  platform::{Adapter, Manager, Peripheral, PeripheralId},
};
use futures::StreamExt;
use libbridgething::{
  DismissReason, Notification, NotificationApp, NotificationCategory, NotificationFlags,
  client::{BridgeToClientNotificationsMsgEvent, NotificationRemoved},
};
use tokio::{
  sync::{Mutex, mpsc},
  task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::net::WireEventBus;

const ANCS_SERVICE: Uuid = Uuid::from_u128(0x7905F431_B5CE_4E99_A40F_4B1E122D00D0);
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

const ACTION_POSITIVE: u8 = 0x00;
const ACTION_NEGATIVE: u8 = 0x01;

const TITLE_MAX: u16 = 256;
const SUBTITLE_MAX: u16 = 256;
const MESSAGE_MAX: u16 = 1024;

const PENDING_QUEUE_CAP: usize = 64;
const COMMAND_MAILBOX_CAP: usize = 16;
const ATTRIBUTE_FETCH_TIMEOUT: Duration = Duration::from_secs(5);
const CONNECT_RETRY_DELAY: Duration = Duration::from_secs(2);
const ANCS_ID_PREFIX: &str = "ancs:";

#[derive(Debug)]
enum AncsCommand {
  Attach { address: Address },
  Detach { address: Address },
  Invoke { uid: u32, action: u8 },
}

#[derive(Debug, Clone)]
pub struct AncsManager {
  tx: mpsc::Sender<AncsCommand>,
}

impl AncsManager {
  pub async fn spawn(bus: WireEventBus) -> AncsResult<(Self, JoinHandle<()>)> {
    let adapter = Arc::new(init_adapter().await?);
    let (tx, rx) = mpsc::channel(COMMAND_MAILBOX_CAP);
    let dispatcher = AncsDispatcher {
      adapter,
      bus,
      rx,
      session: None,
    };
    let handle = tokio::spawn(dispatcher.run());
    Ok((Self { tx }, handle))
  }

  pub async fn attach(&self, address: Address) {
    if self.tx.send(AncsCommand::Attach { address }).await.is_err() {
      tracing::warn!(%address, "ANCS dispatcher closed; cannot attach");
    }
  }

  pub async fn detach(&self, address: Address) {
    if self.tx.send(AncsCommand::Detach { address }).await.is_err() {
      tracing::trace!(%address, "ANCS dispatcher closed; detach no-op");
    }
  }

  pub async fn try_invoke_positive(&self, id: &str) -> bool {
    self.try_invoke(id, ACTION_POSITIVE).await
  }

  pub async fn try_invoke_negative(&self, id: &str) -> bool {
    self.try_invoke(id, ACTION_NEGATIVE).await
  }

  async fn try_invoke(&self, id: &str, action: u8) -> bool {
    let Some(rest) = id.strip_prefix(ANCS_ID_PREFIX) else {
      return false;
    };
    let Ok(uid) = rest.parse::<u32>() else {
      tracing::trace!(%id, "ANCS invoke: malformed UID");
      return true;
    };
    if self.tx.send(AncsCommand::Invoke { uid, action }).await.is_err() {
      tracing::trace!(%id, "ANCS dispatcher closed; invoke dropped");
    }
    true
  }
}

struct AncsDispatcher {
  adapter: Arc<Adapter>,
  bus: WireEventBus,
  rx: mpsc::Receiver<AncsCommand>,
  session: Option<ActiveSession>,
}

async fn init_adapter() -> AncsResult<Adapter> {
  let manager = Manager::new().await?;
  let adapters = manager.adapters().await?;
  adapters.into_iter().next().ok_or(AncsError::NoAdapter)
}

#[derive(Debug)]
struct ActiveSession {
  address: Address,
  invoke_tx: mpsc::Sender<(u32, u8)>,
  cancel: CancellationToken,
  _handle: JoinHandle<()>,
}

impl AncsDispatcher {
  async fn run(mut self) {
    while let Some(cmd) = self.rx.recv().await {
      match cmd {
        AncsCommand::Attach { address } => self.handle_attach(address).await,
        AncsCommand::Detach { address } => self.handle_detach(address).await,
        AncsCommand::Invoke { uid, action } => self.handle_invoke(uid, action).await,
      }
    }
  }

  async fn handle_attach(&mut self, address: Address) {
    if let Some(existing) = self.session.take() {
      tracing::debug!(prev = %existing.address, new = %address, "ANCS replacing active session");
      existing.cancel.cancel();
    }
    let cancel = CancellationToken::new();
    let (invoke_tx, invoke_rx) = mpsc::channel(COMMAND_MAILBOX_CAP);
    let task = AncsSessionTask {
      address,
      adapter: self.adapter.clone(),
      bus: self.bus.clone(),
      invoke_rx,
      cancel: cancel.clone(),
    };
    let handle = tokio::spawn(task.run());
    self.session = Some(ActiveSession {
      address,
      invoke_tx,
      cancel,
      _handle: handle,
    });
  }

  async fn handle_detach(&mut self, address: Address) {
    if let Some(session) = self.session.take_if(|s| s.address == address) {
      tracing::debug!(%address, "ANCS detaching session");
      session.cancel.cancel();
    } else {
      tracing::trace!(%address, "ANCS detach for non-active address; ignoring");
    }
  }

  async fn handle_invoke(&self, uid: u32, action: u8) {
    let Some(session) = &self.session else {
      tracing::trace!(uid, "ANCS invoke: no active session");
      return;
    };
    if session.invoke_tx.send((uid, action)).await.is_err() {
      tracing::trace!(uid, "ANCS invoke: session task closed");
    }
  }
}

struct AncsSessionTask {
  address: Address,
  adapter: Arc<Adapter>,
  bus: WireEventBus,
  invoke_rx: mpsc::Receiver<(u32, u8)>,
  cancel: CancellationToken,
}

impl AncsSessionTask {
  async fn run(self) {
    let AncsSessionTask {
      address,
      adapter,
      bus,
      mut invoke_rx,
      cancel,
    } = self;

    loop {
      if cancel.is_cancelled() {
        return;
      }
      match attempt_session(&adapter, address, &bus, &mut invoke_rx, &cancel).await {
        Ok(()) => return,
        Err(err) if cancel.is_cancelled() => {
          tracing::debug!(%address, ?err, "ANCS session ended after detach");
          return;
        }
        Err(err) => {
          tracing::warn!(%address, ?err, "ANCS session ended; will retry");
          tokio::select! {
            _ = cancel.cancelled() => return,
            _ = tokio::time::sleep(CONNECT_RETRY_DELAY) => {}
          }
        }
      }
    }
  }
}

async fn attempt_session(
  adapter: &Adapter,
  address: Address,
  bus: &WireEventBus,
  invoke_rx: &mut mpsc::Receiver<(u32, u8)>,
  cancel: &CancellationToken,
) -> AncsResult<()> {
  let peripheral = find_peripheral(adapter, address).await?;
  if !peripheral.is_connected().await? {
    peripheral.connect().await?;
  }
  peripheral.discover_services().await?;
  let chars = locate_characteristics(&peripheral)?;

  peripheral.subscribe(&chars.notification_source).await?;
  peripheral.subscribe(&chars.data_source).await?;
  tracing::info!(%address, "ANCS subscribed");

  let mut notifications = peripheral.notifications().await?;

  let pending = Arc::new(Mutex::new(VecDeque::<PendingNotification>::with_capacity(
    PENDING_QUEUE_CAP,
  )));
  let in_flight = Arc::new(Mutex::new(None::<PendingNotification>));
  let mut ds_buffer: Vec<u8> = Vec::with_capacity(2048);

  loop {
    if !cancel.is_cancelled() && in_flight.lock().await.is_none() {
      pump_next(&peripheral, &chars, &pending, &in_flight).await?;
    }

    tokio::select! {
      _ = cancel.cancelled() => {
        let _ = peripheral.disconnect().await;
        return Ok(());
      }
      cmd = invoke_rx.recv() => {
        let Some((uid, action)) = cmd else { return Ok(()); };
        if let Err(err) = write_perform_action(&peripheral, &chars.control_point, uid, action).await {
          tracing::warn!(%address, uid, action, ?err, "ANCS perform-action write failed");
        }
      }
      Some(notif) = notifications.next() => {
        match notif.uuid {
          u if u == NOTIFICATION_SOURCE => {
            handle_ns_frame(&pending, bus, &notif.value).await;
          }
          u if u == DATA_SOURCE => {
            ds_buffer.extend_from_slice(&notif.value);
            try_drain_ds(&mut ds_buffer, &in_flight, bus).await;
          }
          other => {
            tracing::trace!(uuid = %other, "ANCS notify on unexpected characteristic");
          }
        }
      }
      _ = tokio::time::sleep(ATTRIBUTE_FETCH_TIMEOUT), if in_flight.lock().await.is_some() => {
        let stale = in_flight.lock().await.take();
        if let Some(p) = stale {
          tracing::warn!(uid = p.uid, "ANCS attribute fetch timed out; emitting partial");
          emit_notification(bus, &p, NotificationFields::default()).await;
          ds_buffer.clear();
        }
      }
    }
  }
}

async fn find_peripheral(adapter: &Adapter, address: Address) -> AncsResult<Peripheral> {
  // Already-connected peripherals show up in `peripherals()` immediately.
  for p in adapter.peripherals().await? {
    if peripheral_matches(&p, address).await {
      return Ok(p);
    }
  }
  // Otherwise wait for a `DeviceConnected` event for our address.
  let mut events = adapter.events().await?;
  while let Some(event) = events.next().await {
    if let CentralEvent::DeviceConnected(id) = event
      && let Some(p) = peripheral_for_id(adapter, &id).await
      && peripheral_matches(&p, address).await
    {
      return Ok(p);
    }
  }
  Err(AncsError::PeripheralNotFound { address })
}

async fn peripheral_matches(peripheral: &Peripheral, address: Address) -> bool {
  peripheral.address() == bluer_address_to_btleplug(address)
}

async fn peripheral_for_id(adapter: &Adapter, id: &PeripheralId) -> Option<Peripheral> {
  adapter.peripheral(id).await.ok()
}

fn bluer_address_to_btleplug(address: Address) -> btleplug::api::BDAddr {
  btleplug::api::BDAddr::from(address.0)
}

#[derive(Debug)]
struct AncsCharacteristics {
  notification_source: Characteristic,
  control_point: Characteristic,
  data_source: Characteristic,
}

fn locate_characteristics(peripheral: &Peripheral) -> AncsResult<AncsCharacteristics> {
  let mut ns = None;
  let mut cp = None;
  let mut ds = None;
  for service in peripheral.services() {
    if service.uuid != ANCS_SERVICE {
      continue;
    }
    for ch in service.characteristics {
      match ch.uuid {
        u if u == NOTIFICATION_SOURCE => ns = Some(ch),
        u if u == CONTROL_POINT => cp = Some(ch),
        u if u == DATA_SOURCE => ds = Some(ch),
        _ => {}
      }
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

async fn handle_ns_frame(pending: &Arc<Mutex<VecDeque<PendingNotification>>>, bus: &WireEventBus, frame: &[u8]) {
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

  let mut guard = pending.lock().await;
  if guard.len() >= PENDING_QUEUE_CAP {
    let dropped = guard.pop_front();
    if let Some(d) = dropped {
      tracing::warn!(dropped_uid = d.uid, "ANCS pending queue full; dropped oldest");
    }
  }
  guard.push_back(PendingNotification {
    uid,
    event_id,
    flags,
    category,
  });
}

async fn pump_next(
  peripheral: &Peripheral,
  chars: &AncsCharacteristics,
  pending: &Arc<Mutex<VecDeque<PendingNotification>>>,
  in_flight: &Arc<Mutex<Option<PendingNotification>>>,
) -> AncsResult<()> {
  let next = pending.lock().await.pop_front();
  let Some(item) = next else {
    return Ok(());
  };
  let cmd = build_get_attributes(item.uid);
  if let Err(err) = peripheral
    .write(&chars.control_point, &cmd, WriteType::WithResponse)
    .await
  {
    tracing::warn!(uid = item.uid, ?err, "ANCS GNA write failed; dropping");
    return Ok(());
  }
  *in_flight.lock().await = Some(item);
  Ok(())
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

async fn write_perform_action(
  peripheral: &Peripheral,
  control_point: &Characteristic,
  uid: u32,
  action: u8,
) -> AncsResult<()> {
  let mut cmd = Vec::with_capacity(6);
  cmd.push(COMMAND_PERFORM_NOTIFICATION_ACTION);
  cmd.extend_from_slice(&uid.to_le_bytes());
  cmd.push(action);
  peripheral.write(control_point, &cmd, WriteType::WithResponse).await?;
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

async fn try_drain_ds(buffer: &mut Vec<u8>, in_flight: &Arc<Mutex<Option<PendingNotification>>>, bus: &WireEventBus) {
  loop {
    let Some(meta) = in_flight.lock().await.clone() else {
      return;
    };
    let Some((fields, consumed)) = parse_gna_response(buffer) else {
      return;
    };
    buffer.drain(..consumed);
    *in_flight.lock().await = None;
    emit_notification(bus, &meta, fields).await;
  }
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

/// ANCS dates arrive as `yyyyMMdd'T'HHmmss` UTC. Bare-bones parser
/// returning unix epoch seconds; format violations yield `None`.
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
  #[error("no bluetooth adapter available")]
  NoAdapter,
  #[error("peripheral {address} not present in btleplug enumeration")]
  PeripheralNotFound { address: Address },
  #[error("ANCS characteristic {0} not found")]
  CharacteristicMissing(Uuid),
  #[error(transparent)]
  Btleplug(#[from] btleplug::Error),
}
