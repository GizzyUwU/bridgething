use crate::paths::{ON_DEVICE_SENTINEL, is_on_device};

#[cfg(feature = "systemd")]
#[zbus::proxy(
  interface = "org.freedesktop.timedate1",
  default_service = "org.freedesktop.timedate1",
  default_path = "/org/freedesktop/timedate1"
)]
trait Timedate {
  fn set_time(&self, usec_utc: i64, relative: bool, interactive: bool) -> zbus::Result<()>;
  fn set_timezone(&self, name: &str, interactive: bool) -> zbus::Result<()>;
}

#[derive(Debug, thiserror::Error)]
pub enum TimeSysError {
  #[cfg(feature = "systemd")]
  #[error("timedated dbus call failed: {0}")]
  Dbus(#[from] zbus::Error),
  #[error("systemd cargo feature disabled; system-clock control unavailable")]
  #[allow(dead_code)]
  Disabled,
}

pub async fn set_time_unix_s(unix_s: i64) -> Result<(), TimeSysError> {
  if !is_on_device() {
    tracing::warn!(
      unix_s,
      "set_time requested but {ON_DEVICE_SENTINEL} is missing - no-op (off-device safety gate)"
    );
    return Ok(());
  }
  invoke_set_time(unix_s.saturating_mul(1_000_000)).await
}

pub async fn set_timezone(name: &str) -> Result<(), TimeSysError> {
  if !is_on_device() {
    tracing::warn!(
      name,
      "set_timezone requested but {ON_DEVICE_SENTINEL} is missing - no-op (off-device safety gate)"
    );
    return Ok(());
  }
  invoke_set_timezone(name).await
}

#[cfg(feature = "systemd")]
async fn invoke_set_time(usec_utc: i64) -> Result<(), TimeSysError> {
  let conn = zbus::Connection::system().await?;
  let proxy = TimedateProxy::new(&conn).await?;
  proxy.set_time(usec_utc, false, false).await?;
  Ok(())
}

#[cfg(feature = "systemd")]
async fn invoke_set_timezone(name: &str) -> Result<(), TimeSysError> {
  let conn = zbus::Connection::system().await?;
  let proxy = TimedateProxy::new(&conn).await?;
  proxy.set_timezone(name, false).await?;
  Ok(())
}

#[cfg(not(feature = "systemd"))]
async fn invoke_set_time(_usec_utc: i64) -> Result<(), TimeSysError> {
  Err(TimeSysError::Disabled)
}

#[cfg(not(feature = "systemd"))]
async fn invoke_set_timezone(_name: &str) -> Result<(), TimeSysError> {
  Err(TimeSysError::Disabled)
}

/// Convert an iAP2 `(tz_offset_minutes, dst_offset_minutes)` pair to a
/// fixed-offset Olson zone like `Etc/GMT+5`. iAP2 carries no IANA zone
/// name, only a numeric offset, so the synthesised name is the only
/// way to keep glibc-aware time pretty-printing happy. The Etc/GMT+N
/// names invert sign (POSIX historical convention): a +5h zone is
/// `Etc/GMT-5`, a -5h zone is `Etc/GMT+5`.
pub fn fixed_offset_zone_name(tz_offset_minutes: i16, dst_offset_minutes: i8) -> Option<String> {
  let total = i32::from(tz_offset_minutes) + i32::from(dst_offset_minutes);
  if total % 60 != 0 {
    return None;
  }
  let hours = total / 60;
  if !(-12..=14).contains(&hours) {
    return None;
  }
  if hours == 0 {
    return Some("Etc/GMT".to_string());
  }
  Some(format!("Etc/GMT{:+}", -hours))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn fixed_offset_zone_inverts_sign() {
    assert_eq!(fixed_offset_zone_name(0, 0).as_deref(), Some("Etc/GMT"));
    assert_eq!(fixed_offset_zone_name(60, 0).as_deref(), Some("Etc/GMT-1"));
    assert_eq!(fixed_offset_zone_name(-300, 0).as_deref(), Some("Etc/GMT+5"));
    assert_eq!(fixed_offset_zone_name(-300, 60).as_deref(), Some("Etc/GMT+4"));
  }

  #[test]
  fn fixed_offset_zone_rejects_sub_hour() {
    assert!(fixed_offset_zone_name(330, 0).is_none());
  }

  #[test]
  fn fixed_offset_zone_rejects_out_of_range() {
    assert!(fixed_offset_zone_name(-13 * 60, 0).is_none());
    assert!(fixed_offset_zone_name(15 * 60, 0).is_none());
  }
}
