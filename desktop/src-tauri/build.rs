use std::{fs, path::Path};

const PSK: &str = "BRIDGETHING_AUTH_PSK";
const LOCAL_ENV: &str = ".env.local";

const COMCTL32_V6_MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
</assembly>
"#;

fn main() {
  println!("cargo:rerun-if-env-changed={PSK}");
  println!("cargo:rerun-if-changed={LOCAL_ENV}");

  if std::env::var_os(PSK).is_none()
    && let Some(psk) = from_local_env(Path::new(LOCAL_ENV), PSK)
  {
    println!("cargo:rustc-env={PSK}={psk}");
  }

  embed_test_manifest_on_windows();

  tauri_build::build();
}

fn embed_test_manifest_on_windows() {
  if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows")
    || std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc")
  {
    return;
  }
  let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is set for build scripts");
  let manifest = Path::new(&out_dir).join("test-comctl32-v6.manifest");
  fs::write(&manifest, COMCTL32_V6_MANIFEST).expect("the test manifest writes to OUT_DIR");
  println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
  println!("cargo:rustc-link-arg-tests=/MANIFESTINPUT:{}", manifest.display());
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
