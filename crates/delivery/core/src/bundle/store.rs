use std::{
  ffi::OsStr,
  path::{Component, Path, PathBuf},
  sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
  },
};

use bridgething_sdk_runtime::rt;
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use crate::{
  bundle::{
    ASR_MODEL_NAME, BundleArtifact, BundleKind, BundleManifest, BundlePlatform,
    fetch::{ArtifactFetch, DownloadRequest, fetch_json},
  },
  seam::{ArtifactValidator, TransferPolicy},
};

const CURRENT_FILE: &str = "current";
const DEFAULT_ROOT_URL: &str = crate::ota::poll::DEFAULT_OTA_ROOT_URL;
const DEFAULT_CHANNEL: &str = "stable";
const EVENT_CAPACITY: usize = 64;

const NLU_TFLITE_ENTRIES: [&str; 3] = ["manifest.json", "tokenizer.json", "model.tflite"];
const NLU_COREML_ENTRIES: [&str; 3] = ["manifest.json", "tokenizer.json", "model.mlpackage"];
const NLU_ONNX_ENTRIES: [&str; 3] = ["manifest.json", "tokenizer.json", "model.onnx"];

#[derive(Debug, Clone)]
pub struct BundleConfig {
  pub storage_dir: PathBuf,
  pub platform: BundlePlatform,
  pub root_url: String,
  pub channel: String,
}

impl BundleConfig {
  pub fn new(storage_dir: PathBuf, platform: BundlePlatform) -> Self {
    BundleConfig {
      storage_dir,
      platform,
      root_url: DEFAULT_ROOT_URL.to_string(),
      channel: DEFAULT_CHANNEL.to_string(),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundleState {
  Absent,
  Downloading { received: u64, total: u64 },
  Ready { version: String },
  Failed { reason: String },
}

pub struct BundleStore {
  inner: Arc<Inner>,
}

impl BundleStore {
  pub fn new(
    kind: BundleKind,
    config: BundleConfig,
    enabled: bool,
    fetch: Arc<dyn ArtifactFetch>,
    policy: Arc<dyn TransferPolicy>,
    validator: Arc<dyn ArtifactValidator>,
  ) -> Self {
    let root = config.storage_dir.join(format!("bridgething-{}", kind.slug()));
    let (events, _) = broadcast::channel(EVENT_CAPACITY);
    let inner = Arc::new(Inner {
      kind,
      config,
      root,
      fetch,
      policy,
      validator,
      guard: Mutex::new(Guard { enabled, run: None }),
      state: Mutex::new(BundleState::Absent),
      events,
    });

    *inner.state.lock().unwrap() = inner.installed_state();

    BundleStore { inner }
  }

  pub fn state(&self) -> BundleState {
    self.inner.state.lock().unwrap().clone()
  }

  pub fn live(&self) -> Option<PathBuf> {
    if !self.inner.guard.lock().unwrap().enabled {
      return None;
    }
    let installed = self.inner.root.join(self.inner.read_current()?);
    self.inner.require_shape(&installed).ok()?;
    Some(self.inner.resolve(&installed))
  }

  pub fn subscribe(&self) -> broadcast::Receiver<BundleState> {
    self.inner.events.subscribe()
  }

  pub async fn ensure(&self) {
    self.inner.ensure(false).await;
  }

  pub async fn download_now(&self) {
    self.inner.ensure(true).await;
  }

  pub async fn set_enabled(&self, value: bool) {
    let cancelled = {
      let mut guard = self.inner.guard.lock().unwrap();
      if guard.enabled == value {
        return;
      }
      guard.enabled = value;
      if value { None } else { guard.run.take() }
    };

    if value {
      self.inner.publish(self.inner.installed_state());
      let inner = self.inner.clone();
      rt::spawn(async move { inner.ensure(false).await });
      return;
    }

    if let Some(mut run) = cancelled {
      run.cancel.store(true, Ordering::SeqCst);
      let _ = run.done.wait_for(|done| *done).await;
    }
    self.inner.publish(BundleState::Absent);
  }
}

struct Guard {
  enabled: bool,
  run: Option<RunHandle>,
}

#[derive(Clone)]
struct RunHandle {
  cancel: Arc<AtomicBool>,
  forced: bool,
  done: watch::Receiver<bool>,
}

struct Inner {
  kind: BundleKind,
  config: BundleConfig,
  root: PathBuf,
  fetch: Arc<dyn ArtifactFetch>,
  policy: Arc<dyn TransferPolicy>,
  validator: Arc<dyn ArtifactValidator>,
  guard: Mutex<Guard>,
  state: Mutex<BundleState>,
  events: broadcast::Sender<BundleState>,
}

impl Inner {
  async fn ensure(self: &Arc<Self>, forced: bool) {
    let mut waited = false;
    loop {
      let (mut run, settles) = {
        let mut guard = self.guard.lock().unwrap();
        if !guard.enabled {
          return;
        }
        match guard.run.clone() {
          Some(existing) if forced && !existing.forced && !waited => (existing, false),
          Some(existing) => (existing, true),
          None => (self.begin(&mut guard, forced), true),
        }
      };
      let _ = run.done.wait_for(|done| *done).await;
      if settles {
        return;
      }
      waited = true;
    }
  }

  fn begin(self: &Arc<Self>, guard: &mut Guard, forced: bool) -> RunHandle {
    let (done_tx, done_rx) = watch::channel(false);
    let handle = RunHandle {
      cancel: Arc::new(AtomicBool::new(false)),
      forced,
      done: done_rx,
    };
    guard.run = Some(handle.clone());
    let inner = self.clone();
    let cancel = handle.cancel.clone();
    rt::spawn(async move {
      inner.run(&cancel, forced).await;
      inner.guard.lock().unwrap().run = None;
      let _ = done_tx.send(true);
    });
    handle
  }

  async fn run(self: &Arc<Self>, cancel: &AtomicBool, forced: bool) {
    if let Err(reason) = self.attempt(cancel, forced).await {
      if cancel.load(Ordering::SeqCst) {
        return;
      }
      match self.installed_state() {
        installed @ BundleState::Ready { .. } => self.publish(installed),
        _ => self.publish(BundleState::Failed { reason }),
      }
    }
  }

  async fn attempt(self: &Arc<Self>, cancel: &AtomicBool, forced: bool) -> Result<(), String> {
    let manifest: BundleManifest = fetch_json(self.fetch.as_ref(), &self.manifest_url())
      .await
      .map_err(|e| e.to_string())?;
    let artifact = manifest
      .artifact_for(self.config.platform)
      .cloned()
      .ok_or_else(|| format!("the {} manifest carries nothing for this platform", self.kind.slug()))?;

    let installed = self.read_current();
    if installed.as_deref() == Some(manifest.version.as_str())
      && self.require_shape(&self.root.join(&manifest.version)).is_ok()
    {
      self.publish(BundleState::Ready {
        version: manifest.version,
      });
      return Ok(());
    }

    if !forced && !self.policy.allows_large_transfer() {
      self.publish(self.installed_state());
      return Ok(());
    }

    self.install(&manifest.version, &artifact, cancel).await
  }

  async fn install(
    self: &Arc<Self>,
    version: &str,
    artifact: &BundleArtifact,
    cancel: &AtomicBool,
  ) -> Result<(), String> {
    std::fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
    let downloads = self.root.join("downloads");
    let staging = self.root.join(format!("staging-{}", Uuid::now_v7()));

    self.publish(BundleState::Downloading {
      received: 0,
      total: artifact.size,
    });

    let ticker = self.clone();
    let declared = artifact.size;
    let downloaded = self
      .fetch
      .download(DownloadRequest {
        url: artifact.url.clone(),
        dir: downloads.clone(),
        filename: self.kind.download_name().to_string(),
        asset: format!("{} model", self.kind.slug()),
        expected: Some(artifact.digest()),
        progress: Some(Arc::new(move |received, reported| {
          ticker.publish_progress(received, if declared > 0 { declared } else { reported });
        })),
      })
      .await
      .map_err(|e| e.to_string())?;

    if cancel.load(Ordering::SeqCst) {
      return Ok(());
    }

    let staged = {
      let inner = self.clone();
      let target = staging.clone();
      let version = version.to_string();
      rt::spawn_blocking(move || {
        inner
          .stage(&downloaded, &target)
          .and_then(|()| inner.rotate(&target, &version))
      })
      .await
    };
    if let Err(reason) = staged {
      let _ = std::fs::remove_dir_all(&staging);
      return Err(reason);
    }

    let _ = std::fs::remove_dir_all(&downloads);
    self.publish(BundleState::Ready {
      version: version.to_string(),
    });
    Ok(())
  }

  fn stage(&self, downloaded: &Path, staging: &Path) -> Result<(), String> {
    std::fs::create_dir_all(staging).map_err(|e| e.to_string())?;
    self.materialize(downloaded, staging)?;
    self.require_shape(staging)?;
    self
      .validator
      .validate(self.kind.artifact_kind(), &self.resolve(staging))
  }

  fn rotate(&self, staging: &Path, version: &str) -> Result<(), String> {
    let live = self.root.join(version);
    if live.exists() {
      std::fs::remove_dir_all(&live).map_err(|e| e.to_string())?;
    }
    std::fs::rename(staging, &live)
      .map_err(|_| format!("failed to move the staged {} model into place", self.kind.slug()))?;
    std::fs::write(self.root.join(CURRENT_FILE), version).map_err(|e| e.to_string())?;
    self.prune_superseded(version);
    Ok(())
  }

  fn prune_superseded(&self, version: &str) {
    let Ok(entries) = std::fs::read_dir(&self.root) else {
      return;
    };
    for entry in entries.flatten() {
      let name = entry.file_name();
      if name == OsStr::new(version) || name == OsStr::new(CURRENT_FILE) {
        continue;
      }
      let path = entry.path();
      let _ = if path.is_dir() {
        std::fs::remove_dir_all(&path)
      } else {
        std::fs::remove_file(&path)
      };
    }
  }

  fn materialize(&self, downloaded: &Path, staging: &Path) -> Result<(), String> {
    match self.kind {
      BundleKind::Nlu => self.unzip(downloaded, staging),
      BundleKind::Asr => std::fs::copy(downloaded, staging.join(ASR_MODEL_NAME))
        .map(|_| ())
        .map_err(|e| e.to_string()),
    }
  }

  fn unzip(&self, archive: &Path, into: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file)).map_err(|e| e.to_string())?;
    for index in 0..zip.len() {
      let mut entry = zip.by_index(index).map_err(|e| e.to_string())?;
      let name = entry.name().to_string();
      let Some(out) = contained(into, &name) else {
        return Err(format!(
          "{} archive entry escapes the staging root: {name}",
          self.kind.slug()
        ));
      };
      if entry.is_dir() {
        std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        continue;
      }
      if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
      }
      let mut sink = std::fs::File::create(&out).map_err(|e| e.to_string())?;
      std::io::copy(&mut entry, &mut sink).map_err(|e| e.to_string())?;
    }
    Ok(())
  }

  fn require_shape(&self, dir: &Path) -> Result<(), String> {
    match self.kind {
      BundleKind::Nlu => {
        let entries = match self.config.platform {
          BundlePlatform::Android => NLU_TFLITE_ENTRIES,
          BundlePlatform::Ios | BundlePlatform::Macos => NLU_COREML_ENTRIES,
          BundlePlatform::Linux | BundlePlatform::Windows => NLU_ONNX_ENTRIES,
        };
        for entry in entries {
          if !dir.join(entry).exists() {
            return Err(format!("{} archive is missing {entry}", self.kind.slug()));
          }
        }
        Ok(())
      }
      BundleKind::Asr => match std::fs::metadata(dir.join(ASR_MODEL_NAME)) {
        Ok(meta) if meta.is_file() && meta.len() > 0 => Ok(()),
        _ => Err(format!("{} model is missing {ASR_MODEL_NAME}", self.kind.slug())),
      },
    }
  }

  fn resolve(&self, dir: &Path) -> PathBuf {
    match self.kind {
      BundleKind::Nlu => dir.to_path_buf(),
      BundleKind::Asr => dir.join(ASR_MODEL_NAME),
    }
  }

  fn manifest_url(&self) -> String {
    format!(
      "{}/{}/{}/manifest.json",
      self.config.root_url.trim_end_matches('/'),
      self.kind.slug(),
      self.config.channel
    )
  }

  fn installed_state(&self) -> BundleState {
    match self
      .read_current()
      .filter(|version| self.require_shape(&self.root.join(version)).is_ok())
    {
      Some(version) => BundleState::Ready { version },
      None => BundleState::Absent,
    }
  }

  fn read_current(&self) -> Option<String> {
    let marker = std::fs::read_to_string(self.root.join(CURRENT_FILE)).ok()?;
    let version = marker.trim();
    (!version.is_empty()).then(|| version.to_string())
  }

  fn publish(&self, next: BundleState) {
    *self.state.lock().unwrap() = next.clone();
    let _ = self.events.send(next);
  }

  fn publish_progress(&self, received: u64, total: u64) {
    let mut state = self.state.lock().unwrap();
    if !matches!(*state, BundleState::Downloading { .. }) {
      return;
    }
    let next = BundleState::Downloading { received, total };
    *state = next.clone();
    let _ = self.events.send(next);
  }
}

fn contained(root: &Path, name: &str) -> Option<PathBuf> {
  let mut out = root.to_path_buf();
  for component in Path::new(name).components() {
    match component {
      Component::Normal(part) => out.push(part),
      Component::CurDir => {}
      _ => return None,
    }
  }
  out.starts_with(root).then_some(out)
}
