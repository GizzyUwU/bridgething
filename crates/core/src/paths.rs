use std::path::{Path, PathBuf};

pub const ON_DEVICE_SENTINEL: &str = "/etc/superbird";

const ENV_STATE_DIR: &str = "BRIDGETHING_STATE_DIR";
const ENV_WEBAPPS_DIR: &str = "BRIDGETHING_WEBAPPS_DIR";
const ENV_RO_WEBAPPS_DIR: &str = "BRIDGETHING_RO_WEBAPPS_DIR";
const ENV_EXAMPLES_DIR: &str = "BRIDGETHING_EXAMPLES_DIR";

#[cfg(not(debug_assertions))]
const PROD_STATE_DIR: &str = "/var/lib/bridgething/state";
#[cfg(not(debug_assertions))]
const PROD_WEBAPPS_DIR: &str = "/var/bridgething/webapps";
#[cfg(not(debug_assertions))]
const PROD_RO_WEBAPPS_DIR: &str = "/opt/bridgething/webapps";
#[cfg(not(debug_assertions))]
const PROD_EXAMPLES_DIR: &str = "/opt/bridgething/examples";

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
      .join("packages/hub-webapp")
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

pub fn assets_blobs_dir() -> PathBuf {
  state_dir().join("assets")
}
