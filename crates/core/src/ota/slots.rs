use tokio::process::Command;

use crate::paths::{ON_DEVICE_SENTINEL, is_on_device};

#[derive(Debug, thiserror::Error)]
pub enum SlotError {
  #[error("io: {0}")]
  Io(#[from] std::io::Error),
  #[error("fw_printenv {key} exited {status}: {stderr}")]
  PrintEnv { key: String, status: i32, stderr: String },
  #[error("fw_setenv {key}={value} exited {status}: {stderr}")]
  SetEnv {
    key: String,
    value: String,
    status: i32,
    stderr: String,
  },
}

const TRY_MAX: &str = "3";

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Slot {
  A,
  B,
}

impl Slot {
  pub fn selector(self) -> &'static str {
    match self {
      Slot::A => "slot_a",
      Slot::B => "slot_b",
    }
  }

  fn tries_var(self) -> &'static str {
    match self {
      Slot::A => "slot_a_tries",
      Slot::B => "slot_b_tries",
    }
  }
}

pub async fn inactive_slot() -> Result<Slot, SlotError> {
  if !is_on_device() {
    tracing::warn!("inactive_slot off-device ({ON_DEVICE_SENTINEL} missing); defaulting to Slot::B");
    return Ok(Slot::B);
  }
  let active = read_env("slot_active").await.unwrap_or_default();
  Ok(match active.trim() {
    "b" => Slot::A,
    _ => Slot::B,
  })
}

pub async fn confirm_target(slot: Slot) -> Result<(), SlotError> {
  if !is_on_device() {
    tracing::warn!(
      ?slot,
      "confirm_target off-device ({ON_DEVICE_SENTINEL} missing); skipping"
    );
    return Ok(());
  }
  set_env(slot.tries_var(), TRY_MAX).await?;
  Ok(())
}

async fn read_env(key: &str) -> Result<String, SlotError> {
  let out = Command::new("fw_printenv").args(["-n", key]).output().await?;
  if !out.status.success() {
    return Err(SlotError::PrintEnv {
      key: key.into(),
      status: out.status.code().unwrap_or(-1),
      stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    });
  }
  Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

async fn set_env(key: &str, value: &str) -> Result<(), SlotError> {
  let arg = format!("{key}={value}");
  let out = Command::new("fw_setenv").arg(&arg).output().await?;
  if !out.status.success() {
    return Err(SlotError::SetEnv {
      key: key.into(),
      value: value.into(),
      status: out.status.code().unwrap_or(-1),
      stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    });
  }
  Ok(())
}
