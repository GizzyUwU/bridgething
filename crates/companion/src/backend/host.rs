#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct HostClock {
  pub tz_iana: String,
  pub locale: String,
  pub unix_seconds: u64,
  pub utc_offset_minutes: i16,
  pub dst_offset_minutes: i8,
}

#[uniffi::export(with_foreign)]
pub trait HostEnvironment: Send + Sync {
  fn clock(&self) -> HostClock;
}
