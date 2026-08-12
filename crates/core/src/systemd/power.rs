use crate::paths::{ON_DEVICE_SENTINEL, is_on_device};

#[cfg(feature = "systemd")]
#[zbus::proxy(
  interface = "org.freedesktop.systemd1.Manager",
  default_service = "org.freedesktop.systemd1",
  default_path = "/org/freedesktop/systemd1"
)]
trait SystemdManager {
  fn reboot(&self) -> zbus::Result<()>;
  fn power_off(&self) -> zbus::Result<()>;
  fn reset_failed_unit(&self, name: &str) -> zbus::Result<()>;
  fn restart_unit(&self, name: &str, mode: &str) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
}

#[derive(Debug, thiserror::Error)]
pub enum PowerError {
  #[cfg(feature = "systemd")]
  #[error("systemd dbus call failed: {0}")]
  Dbus(#[from] zbus::Error),
  #[cfg(not(feature = "systemd"))]
  #[error("systemd cargo feature disabled; power control unavailable")]
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

pub async fn restart_self() -> Result<(), PowerError> {
  if !is_on_device() {
    tracing::warn!("restart_self requested but {ON_DEVICE_SENTINEL} is missing - no-op (off-device safety gate)");
    return Ok(());
  }
  invoke_restart_self().await
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

#[cfg(feature = "systemd")]
async fn invoke_restart_self() -> Result<(), PowerError> {
  let conn = zbus::Connection::system().await?;
  let proxy = SystemdManagerProxy::new(&conn).await?;
  if let Err(err) = proxy.reset_failed_unit("bridgething.service").await {
    tracing::warn!(
      ?err,
      "reset-failed bridgething.service before restart failed (continuing)"
    );
  }
  proxy.restart_unit("bridgething.service", "replace").await?;
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

#[cfg(not(feature = "systemd"))]
async fn invoke_restart_self() -> Result<(), PowerError> {
  Err(PowerError::Disabled)
}
