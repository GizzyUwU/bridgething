use std::{
  sync::Arc,
  time::{Instant, SystemTime, UNIX_EPOCH},
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
  clock_anchor: Option<Instant>,
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
    let guard = self.inner.read().await;
    let mut info = guard.state.clone();
    if let (Some(clock), Some(anchor)) = (info.wall_clock_unix_s, guard.clock_anchor) {
      let elapsed = u32::try_from(Instant::now().saturating_duration_since(anchor).as_secs()).unwrap_or(u32::MAX);
      info.wall_clock_unix_s = Some(clock.saturating_add(elapsed));
    }
    info
  }

  pub async fn apply_iap2_update(
    &self,
    seconds_since_reference_date: i64,
    tz_offset_minutes: i16,
    dst_offset_minutes: i8,
  ) -> Result<(), TimeError> {
    let standard_offset_minutes = tz_offset_minutes.saturating_sub(i16::from(dst_offset_minutes));
    let synthetic_zone = system_time::fixed_offset_zone_name(standard_offset_minutes, dst_offset_minutes);
    let zone_to_apply = {
      let mut guard = self.inner.write().await;
      guard.state.wall_clock_unix_s = u32::try_from(seconds_since_reference_date).ok();
      guard.clock_anchor = Some(Instant::now());
      guard.state.utc_offset_minutes = Some(standard_offset_minutes);
      guard.state.dst_offset_minutes = Some(dst_offset_minutes);

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
      guard.clock_anchor = info.wall_clock_unix_s.map(|_| Instant::now());
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

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use super::*;

  fn manager() -> TimeManager {
    let (client_man, _listener) = crate::net::create_client_manager();
    TimeManager::new(WireEventBus::new(client_man))
  }

  fn info_at(unix_s: u32) -> TimeInfo {
    TimeInfo {
      tz_iana: Some("America/New_York".into()),
      locale: Some("en_US".into()),
      wall_clock_unix_s: Some(unix_s),
      utc_offset_minutes: Some(-300),
      dst_offset_minutes: Some(60),
    }
  }

  #[tokio::test]
  async fn a_read_carries_the_wall_clock_forward() {
    let time = manager();
    let _ = time.apply_companion_snapshot(info_at(1_785_000_000)).await;

    {
      let mut guard = time.inner.write().await;
      guard.clock_anchor = Instant::now().checked_sub(Duration::from_secs(90));
    }

    let snapshot = time.snapshot().await;
    let clock = snapshot.wall_clock_unix_s.expect("clock");
    assert!(
      (1_785_000_089..=1_785_000_092).contains(&clock),
      "ninety quiet seconds should advance the clock, got {clock}"
    );
  }

  #[tokio::test]
  async fn an_absent_wall_clock_stays_absent() {
    let time = manager();
    let mut info = info_at(0);
    info.wall_clock_unix_s = None;
    let _ = time.apply_companion_snapshot(info).await;

    assert!(time.snapshot().await.wall_clock_unix_s.is_none());
  }

  #[tokio::test]
  async fn an_iap2_update_splits_dst_out_of_the_reported_offset() {
    let time = manager();
    let _ = time.apply_iap2_update(1_785_000_000, -240, 60).await;

    let snapshot = time.snapshot().await;
    assert_eq!(snapshot.utc_offset_minutes, Some(-300));
    assert_eq!(snapshot.dst_offset_minutes, Some(60));
    assert_eq!(
      system_time::fixed_offset_zone_name(
        snapshot.utc_offset_minutes.expect("offset"),
        snapshot.dst_offset_minutes.expect("dst")
      )
      .as_deref(),
      Some("Etc/GMT+4")
    );
  }

  #[tokio::test]
  async fn the_zone_is_untouched_by_the_carry() {
    let time = manager();
    let _ = time.apply_companion_snapshot(info_at(1_785_000_000)).await;

    let snapshot = time.snapshot().await;
    assert_eq!(snapshot.tz_iana.as_deref(), Some("America/New_York"));
    assert_eq!(snapshot.utc_offset_minutes, Some(-300));
    assert_eq!(snapshot.dst_offset_minutes, Some(60));
  }
}
