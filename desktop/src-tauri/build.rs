use std::{fs, path::Path};

const PSK: &str = "BRIDGETHING_AUTH_PSK";
const LOCAL_ENV: &str = ".env.local";

fn main() {
  println!("cargo:rerun-if-env-changed={PSK}");
  println!("cargo:rerun-if-changed={LOCAL_ENV}");

  if std::env::var_os(PSK).is_none()
    && let Some(psk) = from_local_env(Path::new(LOCAL_ENV), PSK)
  {
    println!("cargo:rustc-env={PSK}={psk}");
  }

  tauri_build::build();
}

fn from_local_env(path: &Path, key: &str) -> Option<String> {
  let body = fs::read_to_string(path).ok()?;
  body.lines().find_map(|line| {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
      return None;
    }
    let (name, value) = line.split_once('=')?;
    (name.trim() == key).then(|| value.trim().to_owned())
  })
}
