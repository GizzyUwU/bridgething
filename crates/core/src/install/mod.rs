use std::path::{Path, PathBuf};

use libbridgething::{
  WebappError, WebappInfo, client::BridgeToClientWebappMsgEvent, gateway::BridgeToGatewayWebappMsgEvent,
};

use crate::{
  bluetooth::BluetoothMan,
  net::WireEventBus,
  state::{KvStore, WebappRegistry},
};

pub async fn seed_examples(webapps: &WebappRegistry, examples_dir: &Path, marker: &Path) {
  if tokio::fs::try_exists(marker).await.unwrap_or(false) {
    return;
  }

  match tokio::fs::read_dir(examples_dir).await {
    Ok(mut entries) => {
      let mut zips: Vec<PathBuf> = Vec::new();
      while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("zip") {
          zips.push(path);
        }
      }
      zips.sort();
      for zip in zips {
        match webapps.install_from_path(zip.clone()).await {
          Ok(info) => tracing::info!("seeded example webapp '{}' ({})", info.name, info.id),
          Err(err) => tracing::warn!("failed to seed example {}: {err:?}", zip.display()),
        }
      }
    }
    Err(err) => tracing::debug!("no example seed dir at {}: {err}", examples_dir.display()),
  }

  write_seed_marker(marker).await;
}

async fn write_seed_marker(marker: &Path) {
  if let Some(parent) = marker.parent() {
    let _ = tokio::fs::create_dir_all(parent).await;
  }
  if let Err(err) = tokio::fs::write(marker, b"1").await {
    tracing::warn!("failed to write example seed marker {}: {err:?}", marker.display());
  }
}

pub async fn apply_and_announce(
  webapps: &WebappRegistry,
  kv: &KvStore,
  bus: &WireEventBus,
  bluetooth: &BluetoothMan,
  archive_path: PathBuf,
) -> Result<WebappInfo, WebappError> {
  let info = webapps.install_from_path(archive_path).await?;
  if let Some(manifest) = webapps.manifest(info.id).await
    && let Err(err) = kv.seed_config_defaults(&manifest).await
  {
    tracing::warn!(?err, id = %info.id, "config-default seed failed after install");
  }
  broadcast_installed(bus, bluetooth, info.clone()).await;
  Ok(info)
}

async fn broadcast_installed(bus: &WireEventBus, bluetooth: &BluetoothMan, info: WebappInfo) {
  let gateway_event = BridgeToGatewayWebappMsgEvent::WebappInstalled(info.clone());
  bluetooth.gateway_man.broadcast(gateway_event).await;

  let client_event = BridgeToClientWebappMsgEvent::WebappInstalled(info);
  if let Err(errs) = bus.broadcast_event(client_event).await {
    tracing::debug!(count = errs.len(), "webapp installed client broadcast non-fatal errors");
  }
}

#[cfg(test)]
mod tests {
  use std::io::Write;

  use uuid::Uuid;

  use super::*;

  fn write_bundle_zip(dir: &Path, name: &str, id: &Uuid) -> PathBuf {
    let zip_path = dir.join(format!("{name}.zip"));
    let file = std::fs::File::create(&zip_path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default();
    zip.start_file("index.html", opts).unwrap();
    zip.write_all(b"<!doctype html><title>seed</title>").unwrap();
    let manifest = format!(r#"{{"id":"{id}","name":"{name}","version":"0.1.0","config":[],"permissions":[]}}"#);
    zip.start_file("manifest.json", opts).unwrap();
    zip.write_all(manifest.as_bytes()).unwrap();
    zip.finish().unwrap();
    zip_path
  }

  async fn registry(installed: &Path) -> WebappRegistry {
    let builtin = installed.with_file_name("builtin");
    std::fs::create_dir_all(&builtin).unwrap();
    std::fs::create_dir_all(installed).unwrap();
    WebappRegistry::init(installed.to_path_buf(), builtin).await.unwrap()
  }

  #[tokio::test]
  async fn seeds_once_then_gated_by_marker() {
    let root = std::env::temp_dir().join(format!("bridgething-seed-test-{}", Uuid::now_v7()));
    let examples = root.join("examples");
    let installed = root.join("webapps");
    let marker = root.join(".seeded");
    std::fs::create_dir_all(&examples).unwrap();
    let id_a = Uuid::now_v7();
    let id_b = Uuid::now_v7();
    write_bundle_zip(&examples, "alpha", &id_a);
    write_bundle_zip(&examples, "beta", &id_b);

    let reg = registry(&installed).await;
    seed_examples(&reg, &examples, &marker).await;

    assert!(reg.resolve(id_a).await.is_some(), "alpha seeded");
    assert!(reg.resolve(id_b).await.is_some(), "beta seeded");
    assert!(tokio::fs::try_exists(&marker).await.unwrap(), "marker written");

    let dir_a = installed.join(id_a.simple().to_string());
    tokio::fs::remove_dir_all(&dir_a).await.unwrap();
    reg.rescan().await;
    assert!(reg.resolve(id_a).await.is_none(), "alpha removed");

    seed_examples(&reg, &examples, &marker).await;
    assert!(
      reg.resolve(id_a).await.is_none(),
      "deleted example does not reappear after re-seed"
    );

    let _ = std::fs::remove_dir_all(&root);
  }

  #[tokio::test]
  async fn missing_dir_still_marks_first_boot() {
    let root = std::env::temp_dir().join(format!("bridgething-seed-empty-{}", Uuid::now_v7()));
    let installed = root.join("webapps");
    let marker = root.join(".seeded");
    let reg = registry(&installed).await;

    seed_examples(&reg, &root.join("does-not-exist"), &marker).await;
    assert!(
      tokio::fs::try_exists(&marker).await.unwrap(),
      "marker written even with no seed dir"
    );

    let _ = std::fs::remove_dir_all(&root);
  }
}
