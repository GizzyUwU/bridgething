use std::{
  collections::{BTreeMap, HashMap},
  path::{Component, Path, PathBuf},
  sync::Arc,
};

pub use bridgething_delivery::webapp::{BROWSER_WEBAPP_ID, HUB_WEBAPP_ID, STOCK_WEBAPP_ID};
use libbridgething::{
  ConfigField, WEBAPP_PROVENANCE_MAX_LEN, WebappError, WebappInfo, WebappManifest, WebappRole, WebappSource,
};
use tokio::{
  fs,
  sync::{OnceCell, RwLock},
};
use uuid::Uuid;

use super::{StateResult, storage::WebappProvenanceStore};

const ICON_MAX_BYTES: u64 = 64 * 1024;
pub const SETTINGS_MAX_BYTES: u64 = 1024 * 1024;
pub const OVERLAY_MAX_BYTES: u64 = 512 * 1024;
const EXTRACTED_SIZE_CAP_BYTES: u64 = 1024 * 1024 * 1024;
const RESERVED_BUILTIN_IDS: &[Uuid] = &[STOCK_WEBAPP_ID, HUB_WEBAPP_ID, BROWSER_WEBAPP_ID];
const DEV_SHADOW_NAMESPACE: Uuid = Uuid::from_u128(0x019759e0_dec0_5ade_8000_b71d6e7de5af);

fn is_reserved(id: Uuid) -> bool {
  RESERVED_BUILTIN_IDS.contains(&id)
}

fn bundle_dir_name(id: Uuid) -> String {
  id.simple().to_string()
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
  pub icon_hash: Option<String>,
  pub settings_hash: Option<String>,
  pub overlay_hash: Option<String>,
  pub bundle_hash: Arc<OnceCell<String>>,
  pub provenance: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WebappRegistry {
  installed_root: PathBuf,
  builtin_root: PathBuf,
  provenance: WebappProvenanceStore,
  bundles: Arc<RwLock<HashMap<Uuid, WebappBundle>>>,
}

impl WebappRegistry {
  pub async fn init(
    installed_root: PathBuf,
    builtin_root: PathBuf,
    provenance: WebappProvenanceStore,
  ) -> StateResult<Self> {
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
      provenance,
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
    let provenance = self.provenance.all().await.unwrap_or_else(|err| {
      tracing::warn!(?err, "webapp provenance read failed; treating all as unknown");
      HashMap::new()
    });
    reconcile_installed_names(&self.installed_root).await;
    for path in scan_root(&self.installed_root).await {
      if let Some(bundle) = load_bundle(&path, WebappSource::Installed).await {
        let mut bundle = if is_reserved(bundle.manifest.id) {
          remap_to_dev_shadow(bundle)
        } else {
          bundle
        };
        bundle.provenance = provenance.get(&bundle.manifest.id).cloned();
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

  pub async fn resolve_by_name(&self, spoken: &str) -> Option<Uuid> {
    let needle = normalize_webapp_name(spoken);
    if needle.is_empty() {
      return None;
    }
    let bundles = self.bundles.read().await;
    let candidates: Vec<(Uuid, String)> = bundles
      .values()
      .filter(|b| !matches!(b.manifest.role, WebappRole::Launcher))
      .map(|b| (b.manifest.id, normalize_webapp_name(&b.manifest.name)))
      .collect();

    if let Some((id, _)) = candidates.iter().find(|(_, name)| *name == needle) {
      return Some(*id);
    }

    let mut partial = candidates
      .iter()
      .filter(|(_, name)| !name.is_empty() && (name.contains(&needle) || needle.contains(name.as_str())));
    match (partial.next(), partial.next()) {
      (Some((id, _)), None) => Some(*id),
      (Some(_), Some(_)) => {
        tracing::debug!("webapp name {spoken:?} matches more than one webapp; refusing to guess");
        None
      }
      _ => None,
    }
  }

  pub async fn bundle_hash(&self, id: Uuid) -> Option<String> {
    let (cell, path) = {
      let bundles = self.bundles.read().await;
      let bundle = bundles.get(&id)?;
      (bundle.bundle_hash.clone(), bundle.path.clone())
    };
    Some(cell.get_or_init(|| compute_bundle_hash(path)).await.clone())
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

  pub async fn install_from_path(
    &self,
    archive_path: PathBuf,
    provenance: Option<String>,
  ) -> Result<WebappInfo, WebappError> {
    if let Some(value) = provenance.as_deref()
      && value.len() > WEBAPP_PROVENANCE_MAX_LEN
    {
      return Err(WebappError::ProvenanceTooLong {
        max_bytes: WEBAPP_PROVENANCE_MAX_LEN as u32,
      });
    }

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

    if bundle.manifest.settings.is_some() && bundle.settings_hash.is_none() {
      let _ = fs::remove_dir_all(&staging).await;
      return Err(WebappError::InvalidManifest {
        reason: format!("declared settings page is missing or exceeds {SETTINGS_MAX_BYTES} bytes"),
      });
    }

    if bundle.manifest.overlay.is_some() && bundle.overlay_hash.is_none() {
      let _ = fs::remove_dir_all(&staging).await;
      return Err(WebappError::InvalidManifest {
        reason: format!("declared overlay is missing or exceeds {OVERLAY_MAX_BYTES} bytes"),
      });
    }

    let final_path = self.installed_root.join(bundle_dir_name(bundle.manifest.id));
    swap_into_place(&self.installed_root, &staging, &final_path)
      .await
      .map_err(|e| WebappError::Internal { reason: e.to_string() })?;

    self
      .provenance
      .set(bundle.manifest.id, provenance.as_deref())
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
    let rel = bundle.manifest.icon.as_deref()?;
    bundle.icon_hash.as_ref()?;
    let path = bundle.path.join(rel);
    let bytes = fs::read(&path).await.ok()?;
    Some((bytes, bundle.icon_mime.clone()))
  }

  pub async fn read_settings(&self, id: Uuid) -> Option<Vec<u8>> {
    let bundle = self.bundle(id).await?;
    bundle.settings_hash.as_ref()?;
    let rel = bundle.manifest.settings.as_deref()?;
    let path = bundle.path.join(rel);
    let bytes = fs::read(&path).await.ok()?;
    (bytes.len() as u64 <= SETTINGS_MAX_BYTES).then_some(bytes)
  }

  pub async fn read_overlay(&self, id: Uuid) -> Option<Vec<u8>> {
    let bundle = self.bundle(id).await?;
    bundle.overlay_hash.as_ref()?;
    let rel = bundle.manifest.overlay.as_deref()?;
    let path = bundle.path.join(rel);
    let bytes = fs::read(&path).await.ok()?;
    (bytes.len() as u64 <= OVERLAY_MAX_BYTES).then_some(bytes)
  }

  pub async fn is_launcher(&self, id: Uuid) -> bool {
    matches!(
      self.bundles.read().await.get(&id).map(|b| b.manifest.role),
      Some(WebappRole::Launcher)
    )
  }

  pub async fn provides_overlay(&self, id: Uuid) -> bool {
    self
      .bundles
      .read()
      .await
      .get(&id)
      .is_some_and(|b| b.overlay_hash.is_some())
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
    self.provenance.clear(id).await?;
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

async fn trash_dir(root: &Path, dir: &Path) -> std::io::Result<()> {
  let trash = root.join(format!(".old.{}", Uuid::now_v7().simple()));
  fs::rename(dir, &trash).await?;
  tokio::spawn(async move {
    if let Err(e) = fs::remove_dir_all(&trash).await {
      tracing::warn!("failed to clean old webapp dir {}: {:?}", trash.display(), e);
    }
  });
  Ok(())
}

async fn swap_into_place(root: &Path, staged: &Path, dest: &Path) -> std::io::Result<()> {
  if fs::try_exists(dest).await.unwrap_or(false) {
    trash_dir(root, dest).await?;
  }
  fs::rename(staged, dest).await
}

async fn reconcile_installed_names(root: &Path) {
  let mut paths = scan_root(root).await;
  paths.sort();
  for path in paths {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
      continue;
    };
    let Some(id) = read_manifest_id(&path).await else {
      continue;
    };
    let canonical = bundle_dir_name(id);
    if name == canonical {
      continue;
    }
    let dest = root.join(&canonical);
    if is_valid_bundle(&dest) {
      tracing::warn!("webapp dir '{name}' duplicates {id}, already installed as '{canonical}'; discarding it");
      if let Err(e) = trash_dir(root, &path).await {
        tracing::warn!("could not discard duplicate webapp dir '{name}': {e:?}");
      }
      continue;
    }
    tracing::warn!("webapp dir '{name}' is not the canonical name for {id}; renaming to '{canonical}'");
    if let Err(e) = swap_into_place(root, &path, &dest).await {
      tracing::warn!("could not canonicalize webapp dir '{name}': {e:?}");
    }
  }
}

async fn read_manifest_id(path: &Path) -> Option<Uuid> {
  #[derive(serde::Deserialize)]
  struct IdOnly {
    id: Uuid,
  }
  let bytes = fs::read(path.join("manifest.json")).await.ok()?;
  let parsed = serde_json::from_slice::<IdOnly>(&bytes).ok()?;
  (!parsed.id.is_nil()).then_some(parsed.id)
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

  let (icon_mime, icon_hash) = match manifest.icon.as_deref() {
    Some(rel) => match hash_bundle_file(path, rel, ICON_MAX_BYTES, &dir_name, "icon").await {
      Some(hash) => (Some(guess_mime_from_ext(rel)), Some(hash)),
      None => (None, None),
    },
    None => (None, None),
  };

  let settings_hash = match manifest.settings.as_deref() {
    Some(rel) => hash_bundle_file(path, rel, SETTINGS_MAX_BYTES, &dir_name, "settings page").await,
    None => None,
  };

  let overlay_hash = match manifest.overlay.as_deref() {
    Some(rel) => hash_bundle_file(path, rel, OVERLAY_MAX_BYTES, &dir_name, "overlay").await,
    None => None,
  };

  Some(WebappBundle {
    path: path.to_path_buf(),
    source,
    manifest: Arc::new(manifest),
    icon_mime,
    icon_hash,
    settings_hash,
    overlay_hash,
    bundle_hash: Arc::new(OnceCell::new()),
    provenance: None,
  })
}

async fn hash_bundle_file(root: &Path, rel: &str, cap: u64, dir_name: &str, what: &str) -> Option<String> {
  let p = root.join(rel);
  match fs::metadata(&p).await {
    Ok(meta) if meta.is_file() && meta.len() <= cap => {
      let bytes = fs::read(&p).await.ok()?;
      Some(sha256_hex(&bytes))
    }
    Ok(meta) if meta.is_file() => {
      tracing::warn!(
        "webapp '{}' {} {} is {} bytes (cap {}); ignoring",
        dir_name,
        what,
        p.display(),
        meta.len(),
        cap
      );
      None
    }
    _ => None,
  }
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
  use sha2::{Digest, Sha256};
  let mut hasher = Sha256::new();
  hasher.update(bytes);
  hex::encode(hasher.finalize())
}

async fn compute_bundle_hash(root: PathBuf) -> String {
  use sha2::{Digest, Sha256};
  let mut hasher = Sha256::new();
  let mut entries = collect_files(&root).await;
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
    icon_hash: b.icon_hash.clone(),
    settings_hash: b.settings_hash.clone(),
    overlay_hash: b.overlay_hash.clone(),
    config: b.manifest.config.clone(),
    permissions: b.manifest.permissions.clone(),
    renders_voice_display: b.manifest.renders_voice_display,
    art: b.manifest.art,
    provenance: b.provenance.clone(),
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

fn normalize_webapp_name(raw: &str) -> String {
  const NOISE: &[&str] = &["the", "a", "an", "app", "webapp", "application"];
  raw
    .chars()
    .map(|c| {
      if c.is_alphanumeric() {
        c.to_ascii_lowercase()
      } else {
        ' '
      }
    })
    .collect::<String>()
    .split_whitespace()
    .filter(|w| !NOISE.contains(w))
    .collect::<Vec<_>>()
    .join(" ")
}

#[cfg(test)]
mod tests {
  use super::normalize_webapp_name;

  #[test]
  fn strips_noise_words_and_punctuation() {
    assert_eq!(normalize_webapp_name("the Browser app"), "browser");
    assert_eq!(normalize_webapp_name("Home-Assistant"), "home assistant");
    assert_eq!(normalize_webapp_name("  Hub  "), "hub");
  }

  #[test]
  fn all_noise_normalizes_empty() {
    assert_eq!(normalize_webapp_name("the app"), "");
  }
}
