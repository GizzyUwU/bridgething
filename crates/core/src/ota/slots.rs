//! A/B slot bookkeeping over u-boot env. Owns:
//! - reading the currently-active slot to derive the inactive target
//!   for an OTA install
//! - resetting the new slot's try counter and flipping `want_boot=kernel`
//!   so the next reboot actually attempts the freshly-written image
//!
//! The active-slot bootenv flip itself is in the .swu's sw-description
//! `bootenv:` block - swupdate writes that atomically with the
//! partition writes, so we don't touch `active_slot` here.
//!
//! Mirrors `bridgething-ab.sh`'s `apply` path. We shell out to
//! `fw_printenv` / `fw_setenv` (libubootenv-bin, RDEPENDS'd by the
//! daemon recipe) rather than linking libubootenv directly: same
//! binary the rest of the system uses, no extra crate, easy to debug
//! with the same CLI.

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

/// Try counter the active slot is reset to after a successful install.
/// Three boot attempts before bootloader-side safety swaps back -
/// matches `TRY_MAX` in `bridgething-ab.sh`.
const TRY_MAX: &str = "3";

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Slot {
  A,
  B,
}

impl Slot {
  /// "_a" / "_b" - matches the suffix of `slot_X_try` env vars and the
  /// active_slot value.
  #[allow(dead_code)] // explicitly allowed dead_code so dev builds won't warn
  pub fn suffix(self) -> &'static str {
    match self {
      Slot::A => "_a",
      Slot::B => "_b",
    }
  }

  /// "slot_a" / "slot_b" - matches the sw-description selector key.
  pub fn selector(self) -> &'static str {
    match self {
      Slot::A => "slot_a",
      Slot::B => "slot_b",
    }
  }

  fn try_var(self) -> &'static str {
    match self {
      Slot::A => "slot_a_try",
      Slot::B => "slot_b_try",
    }
  }
}

/// The slot that's NOT currently booted. OTA installs target this one;
/// on success the .swu's bootenv block flips `active_slot` over to it.
pub async fn inactive_slot() -> Result<Slot, SlotError> {
  if !is_on_device() {
    tracing::warn!("inactive_slot off-device ({ON_DEVICE_SENTINEL} missing); defaulting to Slot::B");
    return Ok(Slot::B);
  }
  let active = read_env("active_slot").await.unwrap_or_default();
  Ok(match active.trim() {
    "_b" => Slot::A,
    // Treat empty / unset / "_a" / anything else as "_a is active";
    // matches bridgething-ab.sh's default-to-_a behavior.
    _ => Slot::B,
  })
}

/// Post-install: arm the freshly-written slot for boot. Three tries
/// before bootloader-side safety swaps back, and `want_boot=kernel`
/// so the next reboot actually attempts the new image (default
/// is `burn` for dev iteration safety).
pub async fn confirm_target(slot: Slot) -> Result<(), SlotError> {
  if !is_on_device() {
    tracing::warn!(
      ?slot,
      "confirm_target off-device ({ON_DEVICE_SENTINEL} missing); skipping"
    );
    return Ok(());
  }
  set_env(slot.try_var(), TRY_MAX).await?;
  set_env("want_boot", "kernel").await?;
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
