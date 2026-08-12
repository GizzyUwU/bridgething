use std::{
  io::Write,
  path::Path,
  sync::atomic::{AtomicBool, Ordering},
};

const SERVICE_DIR: &str = "/run/avahi/services";
const SERVICE_FILE_NAME: &str = "bridgething.service";

static PUBLISH_FAILURE_WARNED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, thiserror::Error)]
pub enum AvahiError {
  #[error("avahi service-file write failed: {0}")]
  Io(#[from] std::io::Error),
  #[cfg(feature = "systemd")]
  #[error("avahi reload dbus call failed: {0}")]
  Dbus(#[from] zbus::Error),
  #[cfg(not(feature = "systemd"))]
  #[error("systemd cargo feature disabled; avahi reload unavailable")]
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
  out.push_str("    <type>");
  out.push_str(libbridgething::BRIDGETHING_MDNS_SERVICE_TYPE);
  out.push_str("</type>\n");
  out.push_str("    <port>");
  out.push_str(&libbridgething::BRIDGETHING_NETWORK_GATEWAY_PORT.to_string());
  out.push_str("</port>\n");
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

pub async fn publish_bridgething_service(nickname: Option<&str>) {
  let err = match try_publish(nickname).await {
    Ok(()) => {
      PUBLISH_FAILURE_WARNED.store(false, Ordering::Relaxed);
      return;
    }
    Err(err) => err,
  };

  if PUBLISH_FAILURE_WARNED.swap(true, Ordering::Relaxed) {
    tracing::debug!(?err, "avahi publish still failing");
  } else {
    tracing::warn!(?err, "avahi publish failed; gateway will not be discoverable over mdns");
  }
}

async fn try_publish(nickname: Option<&str>) -> Result<(), AvahiError> {
  write_service_file(Path::new(SERVICE_DIR), nickname)?;
  reload_avahi().await
}

fn write_service_file(dir: &Path, nickname: Option<&str>) -> Result<(), AvahiError> {
  std::fs::create_dir_all(dir)?;
  let xml = render_service_xml(nickname);
  let path = dir.join(SERVICE_FILE_NAME);
  let tmp = path.with_extension("service.tmp");
  {
    let mut f = std::fs::File::create(&tmp)?;
    f.write_all(xml.as_bytes())?;
    f.sync_all()?;
  }
  std::fs::rename(&tmp, &path)?;
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

  #[test]
  fn writes_service_file_creating_missing_dirs() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("avahi/services");

    write_service_file(&dir, Some("Kitchen Thing")).unwrap();

    let written = std::fs::read_to_string(dir.join(SERVICE_FILE_NAME)).unwrap();
    assert_eq!(written, render_service_xml(Some("Kitchen Thing")));
  }

  #[test]
  fn rewrite_replaces_previous_contents_and_leaves_no_temp_file() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path();

    write_service_file(dir, Some("old")).unwrap();
    write_service_file(dir, Some("new")).unwrap();

    let written = std::fs::read_to_string(dir.join(SERVICE_FILE_NAME)).unwrap();
    assert!(written.contains("nickname=new"));
    assert!(!written.contains("nickname=old"));

    let leftovers: Vec<_> = std::fs::read_dir(dir)
      .unwrap()
      .map(|e| e.unwrap().file_name())
      .filter(|name| name != SERVICE_FILE_NAME)
      .collect();
    assert!(leftovers.is_empty(), "unexpected leftovers: {leftovers:?}");
  }

  #[test]
  fn unwritable_service_dir_surfaces_io_error() {
    let root = tempfile::tempdir().unwrap();
    let blocker = root.path().join("blocker");
    std::fs::write(&blocker, b"not a directory").unwrap();

    let err = write_service_file(&blocker.join("services"), None).unwrap_err();
    assert!(matches!(err, AvahiError::Io(_)));
  }
}
