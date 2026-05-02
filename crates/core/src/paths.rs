use std::path::PathBuf;

const ENV_STATE_DIR: &str = "BRIDGETHING_STATE_DIR";
const ENV_WEBAPPS_DIR: &str = "BRIDGETHING_WEBAPPS_DIR";
const ENV_RO_WEBAPPS_DIR: &str = "BRIDGETHING_RO_WEBAPPS_DIR";
const ENV_RUNTIME_DIR: &str = "BRIDGETHING_RUNTIME_DIR";

#[cfg(not(debug_assertions))]
const PROD_STATE_DIR: &str = "/var/lib/bridgething/state";
#[cfg(not(debug_assertions))]
const PROD_WEBAPPS_DIR: &str = "/var/bridgething/webapps";
const PROD_RO_WEBAPPS_DIR: &str = "/usr/share/bridgething/webapps";
#[cfg(not(debug_assertions))]
const PROD_RUNTIME_DIR: &str = "/run/bridgething";

pub fn state_dir() -> PathBuf {
  if let Ok(p) = std::env::var(ENV_STATE_DIR) {
    return PathBuf::from(p);
  }

  #[cfg(debug_assertions)]
  {
    dirs::config_dir()
      .unwrap_or_else(|| PathBuf::from("/tmp"))
      .join("bridgething")
      .join("state")
  }

  #[cfg(not(debug_assertions))]
  PathBuf::from(PROD_STATE_DIR)
}

pub fn webapps_dir() -> PathBuf {
  if let Ok(p) = std::env::var(ENV_WEBAPPS_DIR) {
    return PathBuf::from(p);
  }

  #[cfg(debug_assertions)]
  {
    dirs::data_dir()
      .unwrap_or_else(|| PathBuf::from("/tmp"))
      .join("bridgething")
      .join("webapps")
  }

  #[cfg(not(debug_assertions))]
  PathBuf::from(PROD_WEBAPPS_DIR)
}

pub fn ro_webapps_dir() -> PathBuf {
  if let Ok(p) = std::env::var(ENV_RO_WEBAPPS_DIR) {
    return PathBuf::from(p);
  }
  PathBuf::from(PROD_RO_WEBAPPS_DIR)
}

/// Volatile per-boot directory for state that must NOT survive a
/// reboot (e.g. the chrome-reload-on-restart marker). On the device
/// this is `/run/bridgething/`, a tmpfs path that the kernel wipes on
/// every boot. In dev it's a sub-directory of the OS temp dir, which
/// has the same wipe-on-reboot semantics on both Linux and macOS.
pub fn runtime_dir() -> PathBuf {
  if let Ok(p) = std::env::var(ENV_RUNTIME_DIR) {
    return PathBuf::from(p);
  }

  #[cfg(debug_assertions)]
  {
    std::env::temp_dir().join("bridgething")
  }

  #[cfg(not(debug_assertions))]
  PathBuf::from(PROD_RUNTIME_DIR)
}

/// Path to the marker file used to distinguish "first start since boot"
/// from "restart of an already-running system." If this file is absent,
/// bridgething has not run yet this boot. If it's present, this is a
/// restart and any side effects gated on "did we run before" (notably
/// the chrome reload) should fire.
pub fn restart_marker_path() -> PathBuf {
  runtime_dir().join("started")
}
