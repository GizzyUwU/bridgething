use std::{fs, path::PathBuf};

pub const BOOT_ROLE_FILE: &str = "/var/lib/superbird-usb-role/boot-role";

fn switch() -> Option<PathBuf> {
  fs::read_dir("/sys/class/usb_role")
    .ok()?
    .filter_map(Result::ok)
    .map(|entry| entry.path().join("role"))
    .find(|path| path.exists())
}

pub fn role() -> String {
  switch()
    .and_then(|path| fs::read_to_string(path).ok())
    .map(|role| role.trim().to_string())
    .unwrap_or_else(|| "unavailable".into())
}

pub fn set_role(want: &str) -> Result<(), String> {
  let Some(path) = switch() else {
    return Err("no usb role switch; the kernel or dt does not expose one".into());
  };
  if role() == want {
    return Ok(());
  }
  fs::write(&path, want).map_err(|err| format!("writing {want} to {}: {err}", path.display()))?;
  tracing::info!(role = want, "usb port role set");
  Ok(())
}

pub fn stay_device() -> bool {
  fs::read_to_string(BOOT_ROLE_FILE).is_ok_and(|role| role.trim() == "device")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_escape_hatch_is_the_bsp_helper_rather_than_a_flag_of_our_own() {
    assert_eq!(BOOT_ROLE_FILE, "/var/lib/superbird-usb-role/boot-role");
  }

  #[test]
  fn a_missing_boot_role_means_host_mode_is_wanted() {
    assert!(
      !stay_device(),
      "no persisted role must not read as a request to stay in gadget mode"
    );
  }

  #[test]
  fn a_host_without_a_role_switch_reports_rather_than_panics() {
    if switch().is_none() {
      assert_eq!(role(), "unavailable");
      assert!(set_role("host").is_err());
    }
  }
}
