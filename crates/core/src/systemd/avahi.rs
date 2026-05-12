//! Dynamic avahi service file publisher. The bridgething.service avahi
//! announcement carries a per-device nickname TXT record sourced from
//! the daemon's KV store (`device:nickname`). Static service files in
//! `/etc/avahi/services/` can't host dynamic fields, so the recipe
//! installs `/etc/avahi/services/bridgething.service` as a symlink to
//! `/run/avahi/services/bridgething.service` and this module writes
//! that path on every nickname change (and once at daemon startup).
//!
//! avahi-daemon picks up new / changed files via inotify; a deliberate
//! tmp+rename keeps the writer-vs-reader race off the table. Reload is
//! also wired explicitly via systemd's `ReloadUnit("avahi-daemon")` so
//! a fresh boot doesn't have to wait for the inotify scan to settle.

use std::io::Write;

use crate::paths::{ON_DEVICE_SENTINEL, is_on_device};

const DYNAMIC_SERVICE_PATH: &str = "/run/avahi/services/bridgething.service";
const SERVICE_DIR: &str = "/run/avahi/services";

#[derive(Debug, thiserror::Error)]
pub enum AvahiError {
  #[error("avahi service-file write failed: {0}")]
  Io(#[from] std::io::Error),
  #[cfg(feature = "systemd")]
  #[error("avahi reload dbus call failed: {0}")]
  Dbus(#[from] zbus::Error),
  // explicitly allowed dead_code so production builds don't warn
  #[error("systemd cargo feature disabled; avahi reload unavailable")]
  #[allow(dead_code)]
  Disabled,
}

#[cfg(feature = "systemd")]
#[zbus::proxy(
  interface = "org.freedesktop.systemd1.Manager",
  default_service = "org.freedesktop.systemd1",
  default_path = "/org/freedesktop/systemd1"
)]
trait SystemdManager {
  fn reload_unit(&self, name: &str, mode: &str) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
}

fn render_service_xml(nickname: Option<&str>) -> String {
  let mut out = String::with_capacity(512);
  out.push_str("<?xml version=\"1.0\" standalone='no'?>\n");
  out.push_str("<!DOCTYPE service-group SYSTEM \"avahi-service.dtd\">\n");
  out.push_str("<service-group>\n");
  out.push_str("  <name replace-wildcards=\"yes\">%h Bridgething Gateway</name>\n");
  out.push_str("  <service>\n");
  out.push_str("    <type>_bridgething._tcp</type>\n");
  out.push_str("    <port>8892</port>\n");
  if let Some(value) = nickname {
    out.push_str("    <txt-record>nickname=");
    out.push_str(&xml_escape_text(value));
    out.push_str("</txt-record>\n");
  }
  out.push_str("  </service>\n");
  out.push_str("</service-group>\n");
  out
}

fn xml_escape_text(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  for c in s.chars() {
    match c {
      '&' => out.push_str("&amp;"),
      '<' => out.push_str("&lt;"),
      '>' => out.push_str("&gt;"),
      '"' => out.push_str("&quot;"),
      '\'' => out.push_str("&apos;"),
      _ => out.push(c),
    }
  }
  out
}

pub async fn publish_bridgething_service(nickname: Option<&str>) -> Result<(), AvahiError> {
  if !is_on_device() {
    tracing::debug!(
      "avahi publish skipped: {ON_DEVICE_SENTINEL} missing (nickname={:?})",
      nickname
    );
    return Ok(());
  }
  write_service_file(nickname)?;
  reload_avahi().await?;
  Ok(())
}

fn write_service_file(nickname: Option<&str>) -> Result<(), AvahiError> {
  std::fs::create_dir_all(SERVICE_DIR)?;
  let xml = render_service_xml(nickname);
  let tmp = format!("{DYNAMIC_SERVICE_PATH}.tmp");
  {
    let mut f = std::fs::File::create(&tmp)?;
    f.write_all(xml.as_bytes())?;
    f.sync_all()?;
  }
  std::fs::rename(&tmp, DYNAMIC_SERVICE_PATH)?;
  Ok(())
}

#[cfg(feature = "systemd")]
async fn reload_avahi() -> Result<(), AvahiError> {
  let conn = zbus::Connection::system().await?;
  let proxy = SystemdManagerProxy::new(&conn).await?;
  proxy.reload_unit("avahi-daemon.service", "replace").await?;
  Ok(())
}

#[cfg(not(feature = "systemd"))]
async fn reload_avahi() -> Result<(), AvahiError> {
  Err(AvahiError::Disabled)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn renders_without_nickname() {
    let xml = render_service_xml(None);
    assert!(xml.contains("<port>8892</port>"));
    assert!(!xml.contains("txt-record"));
  }

  #[test]
  fn renders_with_nickname() {
    let xml = render_service_xml(Some("Joey's Car Thing"));
    assert!(xml.contains("<txt-record>nickname=Joey&apos;s Car Thing</txt-record>"));
  }

  #[test]
  fn escapes_xml_special_chars() {
    let xml = render_service_xml(Some("a&b<c>d\"e'f"));
    assert!(xml.contains("nickname=a&amp;b&lt;c&gt;d&quot;e&apos;f"));
  }
}
