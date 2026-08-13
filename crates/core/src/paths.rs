use std::path::{Path, PathBuf};

pub const ON_DEVICE_SENTINEL: &str = "/etc/superbird";

const ENV_STATE_DIR: &str = "BRIDGETHING_STATE_DIR";
const ENV_WEBAPPS_DIR: &str = "BRIDGETHING_WEBAPPS_DIR";
const ENV_RO_WEBAPPS_DIR: &str = "BRIDGETHING_RO_WEBAPPS_DIR";
const ENV_EXAMPLES_DIR: &str = "BRIDGETHING_EXAMPLES_DIR";
const ENV_WAKEWORD_MODEL: &str = "BRIDGETHING_WAKEWORD_MODEL";

#[cfg(not(debug_assertions))]
const PROD_STATE_DIR: &str = "/var/lib/bridgething/state";
#[cfg(not(debug_assertions))]
const PROD_WEBAPPS_DIR: &str = "/var/bridgething/webapps";
#[cfg(not(debug_assertions))]
const PROD_RO_WEBAPPS_DIR: &str = "/opt/bridgething/webapps";
#[cfg(not(debug_assertions))]
const PROD_EXAMPLES_DIR: &str = "/opt/bridgething/examples";
#[cfg(not(debug_assertions))]
const PROD_WAKEWORD_DIR: &str = "/var/bridgething/wakeword";
#[cfg(not(debug_assertions))]
const PROD_WAKEWORD_BASELINE_DIR: &str = "/usr/share/bridgething/wakeword";

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

  #[cfg(debug_assertions)]
  {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
      .join("../../")
      .join("packages/webapps/builtin/hub")
  }

  #[cfg(not(debug_assertions))]
  PathBuf::from(PROD_RO_WEBAPPS_DIR)
}

pub fn examples_dir() -> PathBuf {
  if let Ok(p) = std::env::var(ENV_EXAMPLES_DIR) {
    return PathBuf::from(p);
  }

  #[cfg(debug_assertions)]
  {
    dirs::data_dir()
      .unwrap_or_else(|| PathBuf::from("/tmp"))
      .join("bridgething")
      .join("examples")
  }

  #[cfg(not(debug_assertions))]
  PathBuf::from(PROD_EXAMPLES_DIR)
}

pub fn is_on_device() -> bool {
  Path::new(ON_DEVICE_SENTINEL).exists()
}

pub fn transfers_dir() -> PathBuf {
  state_dir().join("transfers")
}

pub fn bandaid_transfers_dir() -> PathBuf {
  PathBuf::from("/opt/bridgething/.transfers")
}

pub fn partition_free_bytes(path: &Path) -> u64 {
  use std::os::unix::ffi::OsStrExt;
  let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
    return u64::MAX;
  };
  // SAFETY: statvfs takes a NUL-terminated path and writes a POSIX struct into our stack allocation; both pointers are valid for the duration of the call
  let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
  let rc = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };
  if rc != 0 {
    return u64::MAX;
  }
  (stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64)
}

pub fn assets_blobs_dir() -> PathBuf {
  state_dir().join("assets")
}

pub fn wakeword_models() -> Vec<PathBuf> {
  if let Ok(p) = std::env::var(ENV_WAKEWORD_MODEL) {
    return vec![PathBuf::from(p)];
  }

  #[cfg(debug_assertions)]
  {
    vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../wakeword/models/hey_bridgething.btww")]
  }

  #[cfg(not(debug_assertions))]
  vec![
    PathBuf::from(PROD_WAKEWORD_DIR).join("hey_bridgething.btww"),
    PathBuf::from(PROD_WAKEWORD_BASELINE_DIR).join("hey_bridgething.btww"),
  ]
}
