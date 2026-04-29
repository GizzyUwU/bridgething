use libbridgething::{WebappInfo, WebappSource};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use tokio::fs;

use crate::paths;

use super::{StateError, StateResult};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebappManifest {
  #[serde(default)]
  version: Option<String>,
  #[serde(default)]
  description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WebappRegistry {
  /// writable root for user-installed webapps (data partition in prod).
  installed_root: PathBuf,
  /// read-only root for built-in webapps (rootfs in prod - optional).
  builtin_root: PathBuf,
}

impl WebappRegistry {
  pub async fn init() -> StateResult<Self> {
    let installed_root = paths::webapps_dir();
    let builtin_root = paths::ro_webapps_dir();

    if !installed_root.exists() {
      tracing::debug!("creating installed webapps root at {}", installed_root.display());
      fs::create_dir_all(&installed_root).await?;
    }

    tracing::info!(
      "webapp roots: installed={}, builtin={}",
      installed_root.display(),
      builtin_root.display()
    );

    Ok(Self {
      installed_root,
      builtin_root,
    })
  }

  pub fn resolve(&self, name: &str) -> Option<PathBuf> {
    if !is_safe_name(name) {
      return None;
    }
    let installed = self.installed_root.join(name);
    if is_valid_bundle(&installed) {
      return Some(installed);
    }
    let builtin = self.builtin_root.join(name);
    if is_valid_bundle(&builtin) {
      return Some(builtin);
    }
    None
  }

  pub async fn list(&self) -> Vec<WebappInfo> {
    let mut out: BTreeMap<String, WebappInfo> = BTreeMap::new();

    for entry in scan_root(&self.builtin_root).await {
      let info = read_info(&entry, WebappSource::Builtin).await;
      out.insert(info.name.clone(), info);
    }
    for entry in scan_root(&self.installed_root).await {
      let info = read_info(&entry, WebappSource::Installed).await;
      out.insert(info.name.clone(), info);
    }

    out.into_values().collect()
  }

  pub async fn install(&self, name: &str, archive: Vec<u8>) -> StateResult<WebappInfo> {
    if !is_safe_name(name) {
      return Err(StateError::InvalidPath(name.to_string()));
    }

    let final_path = self.installed_root.join(name);
    let staging = self
      .installed_root
      .join(format!("{name}.tmp.{}", uuid::Uuid::now_v7().simple()));

    fs::create_dir_all(&staging).await?;

    let staging_for_unzip = staging.clone();
    let unzip_result = tokio::task::spawn_blocking(move || extract_zip(&archive, &staging_for_unzip))
      .await
      .map_err(|e| StateError::InvalidPath(format!("zip extract task failed: {e}")))?;

    if let Err(e) = unzip_result {
      let _ = fs::remove_dir_all(&staging).await;
      return Err(e);
    }

    if !is_valid_bundle(&staging) {
      let _ = fs::remove_dir_all(&staging).await;
      return Err(StateError::InvalidPath(format!("webapp '{name}' has no index.html")));
    }

    if final_path.exists() {
      let trash = self
        .installed_root
        .join(format!("{name}.old.{}", uuid::Uuid::now_v7().simple()));
      fs::rename(&final_path, &trash).await?;
      tokio::spawn(async move {
        if let Err(e) = fs::remove_dir_all(&trash).await {
          tracing::warn!("failed to clean old webapp dir {}: {:?}", trash.display(), e);
        }
      });
    }

    fs::rename(&staging, &final_path).await?;

    let info = read_info(&final_path, WebappSource::Installed).await;
    Ok(info)
  }

  pub async fn uninstall(&self, name: &str) -> StateResult<bool> {
    if !is_safe_name(name) {
      return Err(StateError::InvalidPath(name.to_string()));
    }

    let path = self.installed_root.join(name);
    if !path.exists() {
      return Ok(false);
    }

    fs::remove_dir_all(&path).await?;
    Ok(true)
  }

  pub fn is_builtin(&self, name: &str) -> bool {
    is_safe_name(name) && is_valid_bundle(&self.builtin_root.join(name))
  }
}

fn is_safe_name(name: &str) -> bool {
  if name.is_empty() {
    return false;
  }
  let candidate = Path::new(name);
  let mut comps = candidate.components();
  let Some(first) = comps.next() else {
    return false;
  };
  if comps.next().is_some() {
    return false;
  }
  matches!(first, Component::Normal(_))
}

fn is_valid_bundle(path: &Path) -> bool {
  path.is_dir() && path.join("index.html").is_file()
}

async fn scan_root(root: &Path) -> Vec<PathBuf> {
  let mut out = Vec::new();
  let Ok(mut read_dir) = fs::read_dir(root).await else {
    return out;
  };
  while let Ok(Some(entry)) = read_dir.next_entry().await {
    let path = entry.path();
    if !path.is_dir() {
      continue;
    }
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
      continue;
    };
    if name.starts_with('.') || name.contains(".tmp.") || name.contains(".old.") {
      continue;
    }
    if is_valid_bundle(&path) {
      out.push(path);
    }
  }
  out
}

async fn read_info(path: &Path, source: WebappSource) -> WebappInfo {
  let name = path
    .file_name()
    .and_then(|n| n.to_str())
    .unwrap_or_default()
    .to_string();
  let manifest = fs::read(path.join("manifest.json"))
    .await
    .ok()
    .and_then(|b| serde_json::from_slice::<WebappManifest>(&b).ok())
    .unwrap_or_default();
  WebappInfo {
    name,
    source,
    version: manifest.version,
    description: manifest.description,
  }
}

fn extract_zip(archive: &[u8], dest: &Path) -> StateResult<()> {
  let cursor = std::io::Cursor::new(archive);
  let mut zip = zip::ZipArchive::new(cursor).map_err(|e| StateError::InvalidPath(format!("zip read failed: {e}")))?;

  for i in 0..zip.len() {
    let mut entry = zip
      .by_index(i)
      .map_err(|e| StateError::InvalidPath(format!("zip entry {i} read failed: {e}")))?;
    let raw_name = entry
      .enclosed_name()
      .ok_or_else(|| StateError::InvalidPath(format!("zip entry {i} has unsafe path")))?;

    let target = dest.join(&raw_name);
    if !target.starts_with(dest) {
      return Err(StateError::InvalidPath(format!(
        "zip entry escapes destination: {}",
        raw_name.display()
      )));
    }

    if entry.is_dir() {
      std::fs::create_dir_all(&target)?;
      continue;
    }

    if let Some(parent) = target.parent() {
      std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::File::create(&target)?;
    std::io::copy(&mut entry, &mut file)?;
  }

  Ok(())
}
