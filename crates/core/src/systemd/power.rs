//! Power-state control via systemd's D-Bus surface. Replaces the prior
//! `sudo reboot` / `sudo shutdown now` shell-outs - same behavior, no
//! suid dependency, structured errors, no shell process spawn per
//! request.
//!
//! Two callers today: webapp-driven `Reboot` / `PowerOff` system
//! commands (`handler::client::system`) and the OTA orchestrator's
//! `RebootFn` thunk (`main::trigger_reboot`).
//!
//! On-device gate: the call is short-circuited to a `tracing::warn`
//! unless `/etc/superbird` exists. That symlink is written by
//! `bridgething-init.service` on first boot, so its presence is a
//! reliable "this is a real Car Thing" signal regardless of debug vs
//! release build, and protects dev hosts that happen to run the daemon
//! as root.

use std::path::Path;

const ON_DEVICE_SENTINEL: &str = "/etc/superbird";

fn is_on_device() -> bool {
  Path::new(ON_DEVICE_SENTINEL).exists()
}

#[cfg(feature = "systemd")]
#[zbus::proxy(
  interface = "org.freedesktop.systemd1.Manager",
  default_service = "org.freedesktop.systemd1",
  default_path = "/org/freedesktop/systemd1"
)]
trait SystemdManager {
  fn reboot(&self) -> zbus::Result<()>;
  fn power_off(&self) -> zbus::Result<()>;
}

#[derive(Debug, thiserror::Error)]
pub enum PowerError {
  #[cfg(feature = "systemd")]
  #[error("systemd dbus call failed: {0}")]
  Dbus(#[from] zbus::Error),
  // explicitly allowed dead_code so production builds don't warn
  #[error("systemd cargo feature disabled; power control unavailable")]
  #[allow(dead_code)]
  Disabled,
}

pub async fn reboot() -> Result<(), PowerError> {
  if !is_on_device() {
    tracing::warn!("reboot requested but {ON_DEVICE_SENTINEL} is missing - no-op (off-device safety gate)");
    return Ok(());
  }
  invoke_reboot().await
}

pub async fn power_off() -> Result<(), PowerError> {
  if !is_on_device() {
    tracing::warn!("power_off requested but {ON_DEVICE_SENTINEL} is missing - no-op (off-device safety gate)");
    return Ok(());
  }
  invoke_power_off().await
}

#[cfg(feature = "systemd")]
async fn invoke_reboot() -> Result<(), PowerError> {
  let conn = zbus::Connection::system().await?;
  let proxy = SystemdManagerProxy::new(&conn).await?;
  proxy.reboot().await?;
  Ok(())
}

#[cfg(feature = "systemd")]
async fn invoke_power_off() -> Result<(), PowerError> {
  let conn = zbus::Connection::system().await?;
  let proxy = SystemdManagerProxy::new(&conn).await?;
  proxy.power_off().await?;
  Ok(())
}

#[cfg(not(feature = "systemd"))]
async fn invoke_reboot() -> Result<(), PowerError> {
  Err(PowerError::Disabled)
}

#[cfg(not(feature = "systemd"))]
async fn invoke_power_off() -> Result<(), PowerError> {
  Err(PowerError::Disabled)
}
