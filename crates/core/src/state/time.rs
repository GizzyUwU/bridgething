//! Wall-clock authority. Holds the most recent `TimeInfo` snapshot
//! the daemon has been told about - by the iAP2 control session
//! (`DeviceTimeUpdate`) when an iPhone is connected, by the gateway
//! companion (`GatewayToBridgeTimeMsg::Snapshot`) otherwise. Webapps
//! query through the Time SDK surface; broadcast happens on every
//! apply, and the system clock is updated through systemd's timedate1
//! D-Bus surface when the new value differs by more than one second
//! from the local clock.

use std::{
  sync::Arc,
  time::{SystemTime, UNIX_EPOCH},
};

use libbridgething::{
  TimeInfo,
  client::{BridgeToClientTimeMsg, TimeSnapshot},
  wire::MsgMeta,
};
use tokio::sync::RwLock;

use crate::{net::WireEventBus, systemd::time as system_time};

const CLOCK_SKEW_THRESHOLD_S: i64 = 1;

#[derive(Debug, Clone)]
pub struct TimeManager {
  inner: Arc<RwLock<Inner>>,
  bus: WireEventBus,
}

#[derive(Debug, Default)]
struct Inner {
  state: TimeInfo,
  last_applied_zone: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum TimeError {
  #[error("broadcast failed for {0} client(s)")]
  Broadcast(usize),
}

impl TimeManager {
  pub fn new(bus: WireEventBus) -> Self {
    Self {
      inner: Arc::new(RwLock::new(Inner::default())),
      bus,
    }
  }

  pub async fn snapshot(&self) -> TimeInfo {
    self.inner.read().await.state.clone()
  }

  pub async fn apply_iap2_update(
    &self,
    seconds_since_reference_date: i64,
    tz_offset_minutes: i16,
    dst_offset_minutes: i8,
  ) -> Result<(), TimeError> {
    let synthetic_zone = system_time::fixed_offset_zone_name(tz_offset_minutes, dst_offset_minutes);
    let zone_to_apply = {
      let mut guard = self.inner.write().await;
      guard.state.wall_clock_unix_s = u32::try_from(seconds_since_reference_date).ok();
      guard.state.utc_offset_minutes = Some(tz_offset_minutes);
      guard.state.dst_offset_minutes = Some(dst_offset_minutes);

      // iAP2 has no IANA name; synthesize a fixed-offset zone so the
      // system clock picks up the user's wall offset. Don't overwrite
      // a previously-applied IANA name from the companion path.
      let existing_iana = guard.state.tz_iana.clone();
      if existing_iana.is_none() {
        match synthetic_zone.as_ref() {
          Some(z) if guard.last_applied_zone.as_deref() != Some(z.as_str()) => Some(z.clone()),
          _ => None,
        }
      } else {
        None
      }
    };

    self.maybe_set_system_clock(seconds_since_reference_date).await;
    if let Some(zone) = zone_to_apply {
      self.maybe_set_timezone(&zone).await;
    }

    self.broadcast().await
  }

  pub async fn apply_companion_snapshot(&self, info: TimeInfo) -> Result<(), TimeError> {
    let (clock_unix_s, zone_to_apply) = {
      let mut guard = self.inner.write().await;
      guard.state = info.clone();
      let clock = info.wall_clock_unix_s.map(i64::from);
      let zone = info
        .tz_iana
        .clone()
        .or_else(|| match (info.utc_offset_minutes, info.dst_offset_minutes) {
          (Some(tz), dst) => system_time::fixed_offset_zone_name(tz, dst.unwrap_or(0)),
          _ => None,
        });
      let zone_to_apply = match zone {
        Some(z) if guard.last_applied_zone.as_deref() != Some(z.as_str()) => Some(z),
        _ => None,
      };
      (clock, zone_to_apply)
    };

    if let Some(unix_s) = clock_unix_s {
      self.maybe_set_system_clock(unix_s).await;
    }
    if let Some(zone) = zone_to_apply {
      self.maybe_set_timezone(&zone).await;
    }

    self.broadcast().await
  }

  async fn maybe_set_system_clock(&self, candidate_unix_s: i64) {
    let now_unix_s = match SystemTime::now().duration_since(UNIX_EPOCH) {
      Ok(d) => d.as_secs() as i64,
      Err(_) => 0,
    };
    if (candidate_unix_s - now_unix_s).abs() < CLOCK_SKEW_THRESHOLD_S {
      tracing::trace!(
        candidate_unix_s,
        now_unix_s,
        "system clock within threshold; skipping SetTime"
      );
      return;
    }
    if let Err(err) = system_time::set_time_unix_s(candidate_unix_s).await {
      tracing::warn!(?err, candidate_unix_s, "failed to set system clock");
    } else {
      tracing::info!(candidate_unix_s, "system clock advanced via timedated.SetTime");
    }
  }

  async fn maybe_set_timezone(&self, zone: &str) {
    if let Err(err) = system_time::set_timezone(zone).await {
      tracing::warn!(?err, zone, "failed to set system timezone");
      return;
    }
    self.inner.write().await.last_applied_zone = Some(zone.to_string());
    tracing::info!(zone, "system timezone applied via timedated.SetTimezone");
  }

  async fn broadcast(&self) -> Result<(), TimeError> {
    let snapshot = self.snapshot().await;
    let event = BridgeToClientTimeMsg::Changed(TimeSnapshot { time: snapshot });
    if let Err(errors) = self.bus.broadcast(event, MsgMeta::Event).await {
      return Err(TimeError::Broadcast(errors.len()));
    }
    Ok(())
  }
}
