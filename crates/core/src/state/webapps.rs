use std::{
  collections::{BTreeMap, HashMap},
  path::{Component, Path, PathBuf},
  sync::Arc,
};

use libbridgething::{ConfigField, WebappError, WebappInfo, WebappManifest, WebappRole, WebappSource};
use tokio::{fs, sync::RwLock};
use uuid::Uuid;

use super::StateResult;

const ICON_MAX_BYTES: u64 = 64 * 1024;
const EXTRACTED_SIZE_CAP_BYTES: u64 = 1024 * 1024 * 1024;
const STOCK_ICON_SVG: &str = include_str!("stock_icon.svg");

pub const STOCK_WEBAPP_ID: Uuid = Uuid::from_u128(0xb12b_e731_416c_4cf7_8a91_3d2f_19a4_5e21);
pub const HUB_WEBAPP_ID: Uuid = Uuid::from_u128(0x019693c0_5c6a_71f0_a89d_7e2a4d9c0a01);
const RESERVED_BUILTIN_IDS: &[Uuid] = &[STOCK_WEBAPP_ID, HUB_WEBAPP_ID];
const DEV_SHADOW_NAMESPACE: Uuid = Uuid::from_u128(0x019759e0_dec0_5ade_8000_b71d6e7de5af);

fn is_reserved(id: Uuid) -> bool {
  RESERVED_BUILTIN_IDS.contains(&id)
}

fn dev_shadow_id(reserved: Uuid) -> Uuid {
  Uuid::new_v5(&DEV_SHADOW_NAMESPACE, reserved.as_bytes())
}

#[derive(Debug, Clone)]
pub struct WebappBundle {
  pub path: PathBuf,
  pub source: WebappSource,
  pub manifest: Arc<WebappManifest>,
  pub icon_mime: Option<String>,
  pub icon_size: Option<u64>,
  pub bundle_hash: String,
}

#[derive(Debug, Clone)]
pub struct WebappRegistry {
  installed_root: PathBuf,
  builtin_root: PathBuf,
  bundles: Arc<RwLock<HashMap<Uuid, WebappBundle>>>,
}

impl WebappRegistry {
  pub async fn init(installed_root: PathBuf, builtin_root: PathBuf) -> StateResult<Self> {
    if !installed_root.exists() {
      tracing::debug!("creating installed webapps root at {}", installed_root.display());
      fs::create_dir_all(&installed_root).await?;
    }

    tracing::info!(
      "webapp roots: installed={}, builtin={}",
      installed_root.display(),
      builtin_root.display()
    );

    let me = Self {
      installed_root,
      builtin_root,
      bundles: Arc::new(RwLock::new(HashMap::new())),
    };
    me.rescan().await;
    Ok(me)
  }

  pub async fn rescan(&self) {
    let mut bundles: HashMap<Uuid, WebappBundle> = HashMap::new();
    for path in scan_root(&self.builtin_root).await {
      if let Some(bundle) = load_bundle(&path, WebappSource::Builtin).await
        && bundles.insert(bundle.manifest.id, bundle).is_some()
      {
        tracing::warn!("duplicate webapp uuid in builtin root: {}", path.display());
      }
    }
    for path in scan_root(&self.installed_root).await {
      if let Some(bundle) = load_bundle(&path, WebappSource::Installed).await {
        let bundle = if is_reserved(bundle.manifest.id) {
          remap_to_dev_shadow(bundle)
        } else {
          bundle
        };
        if let Some(prev) = bundles.insert(bundle.manifest.id, bundle.clone()) {
          tracing::debug!(
            "installed webapp '{}' shadows existing entry at {}",
            bundle.path.display(),
            prev.path.display()
          );
        }
      }
    }
    tracing::debug!("webapp registry: {} bundles loaded", bundles.len());
    *self.bundles.write().await = bundles;
  }

  pub async fn resolve(&self, id: Uuid) -> Option<PathBuf> {
    self.bundles.read().await.get(&id).map(|b| b.path.clone())
  }

  pub async fn bundle_hash(&self, id: Uuid) -> Option<String> {
    self.bundles.read().await.get(&id).map(|b| b.bundle_hash.clone())
  }

  pub async fn launcher_id(&self) -> Option<Uuid> {
    let bundles = self.bundles.read().await;
    bundles
      .values()
      .find(|b| matches!(b.manifest.role, WebappRole::Launcher))
      .map(|b| b.manifest.id)
  }

  pub async fn bundle(&self, id: Uuid) -> Option<WebappBundle> {
    self.bundles.read().await.get(&id).cloned()
  }

  pub async fn manifest(&self, id: Uuid) -> Option<Arc<WebappManifest>> {
    self.bundles.read().await.get(&id).map(|b| b.manifest.clone())
  }

  pub async fn list(&self) -> Vec<WebappInfo> {
    let bundles = self.bundles.read().await;
    let mut infos: BTreeMap<String, WebappInfo> = BTreeMap::new();
    for b in bundles.values() {
      let info = bundle_to_info(b);
      infos.insert(format!("{}-{}", info.name, info.id.simple()), info);
    }
    infos.into_values().collect()
  }

  pub async fn list_for_clients(&self) -> Vec<WebappInfo> {
    self
      .list()
      .await
      .into_iter()
      .filter(|info| !matches!(info.role, WebappRole::Launcher))
      .collect()
  }

  pub async fn is_builtin(&self, id: Uuid) -> bool {
    matches!(
      self.bundles.read().await.get(&id).map(|b| b.source),
      Some(WebappSource::Builtin)
    )
  }

  pub async fn default_id(&self) -> Option<Uuid> {
    let bundles = self.bundles.read().await;
    if bundles.contains_key(&HUB_WEBAPP_ID) {
      return Some(HUB_WEBAPP_ID);
    }

    if bundles.contains_key(&STOCK_WEBAPP_ID) {
      return Some(STOCK_WEBAPP_ID);
    }
    bundles
      .values()
      .find(|b| matches!(b.source, WebappSource::Builtin))
      .map(|b| b.manifest.id)
  }

  pub async fn install_from_path(&self, archive_path: PathBuf) -> Result<WebappInfo, WebappError> {
    let staging = self.installed_root.join(format!(".tmp.{}", Uuid::now_v7().simple()));
    fs::create_dir_all(&staging)
      .await
      .map_err(|e| WebappError::Internal { reason: e.to_string() })?;

    let staging_for_unzip = staging.clone();
    let extract_result = tokio::task::spawn_blocking(move || extract_zip(&archive_path, &staging_for_unzip))
      .await
      .unwrap_or_else(|e| {
        Err(WebappError::Internal {
          reason: format!("zip extract task panicked: {e}"),
        })
      });
    if let Err(e) = extract_result {
      let _ = fs::remove_dir_all(&staging).await;
      return Err(e);
    }

    if !is_valid_bundle(&staging) {
      let _ = fs::remove_dir_all(&staging).await;
      return Err(WebappError::MissingIndexHtml);
    }

    let bundle = match load_bundle(&staging, WebappSource::Installed).await {
      Some(b) if !b.manifest.id.is_nil() => b,
      _ => {
        let _ = fs::remove_dir_all(&staging).await;
        return Err(WebappError::InvalidManifest {
          reason: "manifest.json missing, unparseable, or failed schema validation".into(),
        });
      }
    };

    if is_reserved(bundle.manifest.id) {
      let _ = fs::remove_dir_all(&staging).await;
      return Err(WebappError::IdReserved {
        id: bundle.manifest.id.to_string(),
      });
    }

    let final_dir_name = bundle.manifest.id.simple().to_string();
    let final_path = self.installed_root.join(&final_dir_name);

    if final_path.exists() {
      let trash = self.installed_root.join(format!(".old.{}", Uuid::now_v7().simple()));
      fs::rename(&final_path, &trash)
        .await
        .map_err(|e| WebappError::Internal { reason: e.to_string() })?;
      tokio::spawn(async move {
        if let Err(e) = fs::remove_dir_all(&trash).await {
          tracing::warn!("failed to clean old webapp dir {}: {:?}", trash.display(), e);
        }
      });
    }

    fs::rename(&staging, &final_path)
      .await
      .map_err(|e| WebappError::Internal { reason: e.to_string() })?;

    self.rescan().await;
    let installed = self.bundle(bundle.manifest.id).await.ok_or(WebappError::Internal {
      reason: "post-install scan dropped the bundle".into(),
    })?;
    Ok(bundle_to_info(&installed))
  }

  pub async fn read_icon(&self, id: Uuid) -> Option<(Vec<u8>, Option<String>)> {
    let bundle = self.bundle(id).await?;
    if id == STOCK_WEBAPP_ID {
      return Some((STOCK_ICON_SVG.as_bytes().to_vec(), Some("image/svg+xml".to_string())));
    }
    let rel = bundle.manifest.icon.as_deref()?;
    bundle.icon_size?;
    let path = bundle.path.join(rel);
    let bytes = fs::read(&path).await.ok()?;
    Some((bytes, bundle.icon_mime.clone()))
  }

  pub async fn uninstall(&self, id: Uuid) -> StateResult<bool> {
    let bundle = match self.bundle(id).await {
      Some(b) => b,
      None => return Ok(false),
    };
    if !matches!(bundle.source, WebappSource::Installed) {
      return Ok(false);
    }
    fs::remove_dir_all(&bundle.path).await?;
    self.rescan().await;
    Ok(true)
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
    if name.starts_with('.') {
      continue;
    }
    if is_valid_bundle(&path) {
      out.push(path);
    }
  }
  out
}

async fn load_bundle(path: &Path, source: WebappSource) -> Option<WebappBundle> {
  if !is_valid_bundle(path) {
    return None;
  }
  let dir_name = path.file_name().and_then(|n| n.to_str())?.to_string();
  if !is_safe_name(&dir_name) {
    return None;
  }

  let manifest = match fs::read(path.join("manifest.json")).await {
    Ok(bytes) => match serde_json::from_slice::<WebappManifest>(&bytes) {
      Ok(m) => match validate_manifest(&m) {
        Ok(()) => m,
        Err(e) => {
          tracing::warn!("webapp '{dir_name}' manifest invalid ({e}); skipping");
          return None;
        }
      },
      Err(e) => {
        tracing::warn!("webapp '{dir_name}' manifest.json failed to parse ({e}); skipping");
        return None;
      }
    },
    Err(_) => {
      tracing::warn!("webapp '{dir_name}' has no manifest.json; skipping");
      return None;
    }
  };

  let (icon_mime, icon_size) = match manifest.icon.as_deref() {
    Some(rel) => {
      let p = path.join(rel);
      match fs::metadata(&p).await {
        Ok(meta) if meta.is_file() && meta.len() <= ICON_MAX_BYTES => {
          (Some(guess_mime_from_ext(rel)), Some(meta.len()))
        }
        Ok(meta) if meta.is_file() => {
          tracing::warn!(
            "webapp '{}' icon {} is {} bytes (cap {}); ignoring",
            dir_name,
            p.display(),
            meta.len(),
            ICON_MAX_BYTES
          );
          (None, None)
        }
        _ => (None, None),
      }
    }
    None => (None, None),
  };

  let bundle_hash = compute_bundle_hash(path).await;

  let (icon_mime, icon_size) = if manifest.id == STOCK_WEBAPP_ID {
    (Some("image/svg+xml".to_string()), Some(STOCK_ICON_SVG.len() as u64))
  } else {
    (icon_mime, icon_size)
  };

  Some(WebappBundle {
    path: path.to_path_buf(),
    source,
    manifest: Arc::new(manifest),
    icon_mime,
    icon_size,
    bundle_hash,
  })
}

async fn compute_bundle_hash(root: &Path) -> String {
  use sha2::{Digest, Sha256};
  let mut hasher = Sha256::new();
  let mut entries = collect_files(root).await;
  entries.sort();
  for rel in entries {
    let abs = root.join(&rel);
    if let Ok(bytes) = fs::read(&abs).await {
      hasher.update(rel.to_string_lossy().as_bytes());
      hasher.update([0u8]);
      hasher.update(&bytes);
      hasher.update([0u8]);
    }
  }
  let digest = hasher.finalize();
  hex::encode(&digest[..8])
}

async fn collect_files(root: &Path) -> Vec<PathBuf> {
  let mut out = Vec::new();
  let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
  while let Some(dir) = stack.pop() {
    let Ok(mut rd) = fs::read_dir(&dir).await else {
      continue;
    };
    while let Ok(Some(entry)) = rd.next_entry().await {
      let path = entry.path();
      if path.is_dir() {
        stack.push(path);
      } else if let Ok(rel) = path.strip_prefix(root) {
        out.push(rel.to_path_buf());
      }
    }
  }
  out
}

fn validate_manifest(m: &WebappManifest) -> Result<(), String> {
  if m.id.is_nil() {
    return Err("manifest id is nil".into());
  }
  if m.name.trim().is_empty() {
    return Err("manifest name is empty".into());
  }
  if m.version.trim().is_empty() {
    return Err("manifest version is empty".into());
  }
  let mut seen = std::collections::HashSet::new();
  for f in &m.config {
    let key = f.key();
    if key.trim().is_empty() {
      return Err("config field key is empty".into());
    }
    if !seen.insert(key) {
      return Err(format!("duplicate config key '{key}'"));
    }
    validate_config_field(f)?;
  }
  Ok(())
}

fn validate_config_field(field: &ConfigField) -> Result<(), String> {
  match field {
    ConfigField::Number(f) => {
      if let Some(d) = f.default {
        if let Some(min) = f.min
          && d < min
        {
          return Err(format!("default for '{}' below min", f.key));
        }
        if let Some(max) = f.max
          && d > max
        {
          return Err(format!("default for '{}' above max", f.key));
        }
      }
    }
    ConfigField::Enum(f) => {
      if f.choices.is_empty() {
        return Err(format!("enum '{}' has no choices", f.key));
      }
      if let Some(d) = &f.default
        && !f.choices.contains(d)
      {
        return Err(format!("default '{d}' for '{}' not in choices", f.key));
      }
    }
    _ => {}
  }
  Ok(())
}

fn guess_mime_from_ext(name: &str) -> String {
  let ext = Path::new(name)
    .extension()
    .and_then(|s| s.to_str())
    .unwrap_or("")
    .to_ascii_lowercase();
  match ext.as_str() {
    "png" => "image/png",
    "jpg" | "jpeg" => "image/jpeg",
    "svg" => "image/svg+xml",
    "webp" => "image/webp",
    "gif" => "image/gif",
    _ => "application/octet-stream",
  }
  .to_string()
}

fn remap_to_dev_shadow(bundle: WebappBundle) -> WebappBundle {
  let original_id = bundle.manifest.id;
  let shadow_id = dev_shadow_id(original_id);
  let mut manifest = (*bundle.manifest).clone();
  manifest.id = shadow_id;
  if matches!(manifest.role, WebappRole::Launcher) {
    manifest.role = WebappRole::Standard;
  }
  tracing::info!(
    "installed bundle at {} claims reserved id {}; mapped to dev shadow {}",
    bundle.path.display(),
    original_id,
    shadow_id,
  );
  WebappBundle {
    manifest: Arc::new(manifest),
    ..bundle
  }
}

fn bundle_to_info(b: &WebappBundle) -> WebappInfo {
  WebappInfo {
    id: b.manifest.id,
    name: b.manifest.name.clone(),
    source: b.source,
    role: b.manifest.role,
    version: b.manifest.version.clone(),
    description: b.manifest.description.clone(),
    icon_available: b.icon_size.is_some(),
    icon_mime: b.icon_mime.clone(),
    config: b.manifest.config.clone(),
    permissions: b.manifest.permissions.clone(),
    voice_grammar: b.manifest.voice_grammar.clone(),
  }
}

pub(crate) fn extract_zip(archive_path: &Path, dest: &Path) -> Result<(), WebappError> {
  let file = std::fs::File::open(archive_path).map_err(|e| WebappError::Internal {
    reason: format!("open archive: {e}"),
  })?;
  let mut zip = zip::ZipArchive::new(file).map_err(|e| WebappError::ZipMalformed {
    reason: format!("zip read failed: {e}"),
  })?;
  let mut extracted_total: u64 = 0;

  for i in 0..zip.len() {
    let mut entry = zip.by_index(i).map_err(|e| WebappError::ZipMalformed {
      reason: format!("zip entry {i} read failed: {e}"),
    })?;
    let raw_name = entry.enclosed_name().ok_or_else(|| WebappError::ZipMalformed {
      reason: format!("zip entry {i} has unsafe path"),
    })?;

    let target = dest.join(&raw_name);
    if !target.starts_with(dest) {
      return Err(WebappError::ZipMalformed {
        reason: format!("zip entry escapes destination: {}", raw_name.display()),
      });
    }

    if entry.is_dir() {
      std::fs::create_dir_all(&target).map_err(|e| WebappError::Internal { reason: e.to_string() })?;
      continue;
    }

    let declared_size = entry.size();
    let prospective_total = extracted_total.saturating_add(declared_size);
    if prospective_total > EXTRACTED_SIZE_CAP_BYTES {
      return Err(WebappError::ExtractedTooLarge {
        max_bytes: EXTRACTED_SIZE_CAP_BYTES.min(u32::MAX as u64) as u32,
      });
    }

    if let Some(parent) = target.parent() {
      std::fs::create_dir_all(parent).map_err(|e| WebappError::Internal { reason: e.to_string() })?;
    }
    let mut out = std::fs::File::create(&target).map_err(|e| WebappError::Internal { reason: e.to_string() })?;
    let mut bounded = std::io::Read::take(&mut entry, declared_size + 1);
    let copied = std::io::copy(&mut bounded, &mut out).map_err(|e| WebappError::Internal { reason: e.to_string() })?;
    if copied != declared_size {
      return Err(WebappError::ZipMalformed {
        reason: format!("entry {i} size mismatch: CD says {declared_size}, decompressed {copied}"),
      });
    }
    extracted_total = extracted_total.saturating_add(copied);
  }

  Ok(())
}
