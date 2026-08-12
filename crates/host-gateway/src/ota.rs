use std::{
  collections::BTreeMap,
  path::{Path, PathBuf},
  sync::Arc,
};

use anyhow::{Context, Result, anyhow};
use bridgething_delivery::{
  ota::{
    event::OtaPhaseSnapshot,
    service::{BandaidArtifact, IMAGE_SWU_ASSET},
    stream::{Artifact, FileSource},
  },
  session::DeliverySession,
};
use libbridgething::{
  OtaKind,
  gateway::{OtaPatch, OtaPatchAlgorithm},
};

use crate::{
  chaos::ChaosConfig,
  session::{self, DEVICE_ID},
};

const PATCH_LEVEL: i32 = 19;
const PATCH_WINDOW_LOG: u32 = 27;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushShape {
  Whole,
  PatchFrom(PathBuf),
  Compressed,
}

pub struct PushRequest {
  pub kind: OtaKind,
  pub artifact: PathBuf,
  pub shape: PushShape,
  pub update_url_base: Option<String>,
  pub zcks: BTreeMap<String, PathBuf>,
  pub version: Option<String>,
}

pub async fn run_push(url: &str, chaos: ChaosConfig, request: PushRequest) -> Result<()> {
  let session = session::connect(url, chaos).await?;
  push(&session, request).await
}

pub async fn push(session: &DeliverySession, request: PushRequest) -> Result<()> {
  let (artifact, patch) = match &request.shape {
    PushShape::Whole => (request.artifact.clone(), None),
    PushShape::PatchFrom(source) => {
      let (path, spec) = cut_patch(source, &request.artifact).await?;
      (path, Some(spec))
    }
    PushShape::Compressed => {
      let (path, spec) = compress(&request.artifact).await?;
      (path, Some(spec))
    }
  };

  for (asset, path) in &request.zcks {
    let meta = tokio::fs::metadata(path)
      .await
      .with_context(|| format!("stat .zck {}", path.display()))?;
    tracing::info!(%asset, path = %path.display(), size = meta.len(), "armed for range serving");
  }

  let terminal = match request.kind {
    OtaKind::Image => {
      session
        .ota
        .push_update(
          DEVICE_ID,
          Arc::new(FileSource::open(artifact)),
          request
            .zcks
            .into_iter()
            .map(|(asset, path)| (asset, Arc::new(FileSource::open(path)) as Arc<dyn Artifact>))
            .collect(),
          request.update_url_base.as_deref(),
        )
        .await
    }
    kind => {
      session
        .ota
        .push_bandaid_batch(
          DEVICE_ID,
          vec![BandaidArtifact {
            kind,
            artifact: Arc::new(FileSource::open(artifact)),
            label: label_for(kind).into(),
            patch,
            version: request.version.clone(),
          }],
        )
        .await
    }
  };

  report(terminal)
}

fn label_for(kind: OtaKind) -> &'static str {
  match kind {
    OtaKind::Image => IMAGE_SWU_ASSET,
    OtaKind::Daemon => "daemon",
    OtaKind::BuiltinWebapp | OtaKind::InstalledWebapp => "webapp",
    OtaKind::WakewordModel => "wakeword",
  }
}

fn report(terminal: OtaPhaseSnapshot) -> Result<()> {
  match terminal {
    OtaPhaseSnapshot::Completed => {
      tracing::info!("update applied");
      Ok(())
    }
    OtaPhaseSnapshot::Failed { reason } => Err(anyhow!("update failed: {reason}")),
    other => Err(anyhow!("update ended on {other:?}")),
  }
}

async fn compress(target: &Path) -> Result<(PathBuf, OtaPatch)> {
  let result_sha256 = hash_file(target).await?;
  let result_size = artifact_size(target).await?;

  let out = std::env::temp_dir().join(format!("bridgething-full-{}.zst", uuid::Uuid::now_v7()));
  let (src, dst) = (target.to_path_buf(), out.clone());
  tokio::task::spawn_blocking(move || write_zstd(&src, None, &dst))
    .await
    .map_err(|err| anyhow!("compress task join: {err}"))??;

  let len = tokio::fs::metadata(&out).await?.len();
  tracing::info!(
    artifact = %out.display(),
    compressed_len = len,
    result_size,
    ratio = format!("{:.1}%", 100.0 * len as f64 / result_size as f64),
    "compressed artifact with plain zstd"
  );
  Ok((
    out,
    OtaPatch {
      algorithm: OtaPatchAlgorithm::Zstd,
      result_sha256,
      result_size,
      source_sha256: None,
    },
  ))
}

async fn cut_patch(source: &Path, target: &Path) -> Result<(PathBuf, OtaPatch)> {
  let source_sha256 = hash_file(source).await?;
  let result_sha256 = hash_file(target).await?;
  let result_size = artifact_size(target).await?;

  let out = std::env::temp_dir().join(format!("bridgething-patch-{}.zst", uuid::Uuid::now_v7()));
  let (src, tgt, dst) = (source.to_path_buf(), target.to_path_buf(), out.clone());
  tokio::task::spawn_blocking(move || write_zstd(&tgt, Some(&src), &dst))
    .await
    .map_err(|err| anyhow!("patch task join: {err}"))??;

  let patch_len = tokio::fs::metadata(&out).await?.len();
  tracing::info!(
    source = %source.display(),
    patch = %out.display(),
    patch_len,
    result_size,
    ratio = format!("{:.1}%", 100.0 * patch_len as f64 / result_size as f64),
    "cut delta patch"
  );

  Ok((
    out,
    OtaPatch {
      algorithm: OtaPatchAlgorithm::ZstdPatchFrom,
      result_sha256,
      result_size,
      source_sha256: Some(source_sha256),
    },
  ))
}

fn write_zstd(target: &Path, prefix: Option<&Path>, out: &Path) -> Result<()> {
  let file = std::fs::File::create(out).with_context(|| format!("create {}", out.display()))?;
  let prefix = prefix
    .map(|source| std::fs::read(source).with_context(|| format!("read patch source {}", source.display())))
    .transpose()?;
  let mut encoder = match &prefix {
    Some(bytes) => zstd::stream::write::Encoder::with_ref_prefix(file, PATCH_LEVEL, bytes)?,
    None => zstd::stream::write::Encoder::new(file, PATCH_LEVEL)?,
  };
  encoder.set_parameter(zstd::zstd_safe::CParameter::EnableLongDistanceMatching(true))?;
  encoder.set_parameter(zstd::zstd_safe::CParameter::WindowLog(PATCH_WINDOW_LOG))?;
  let mut reader =
    std::io::BufReader::new(std::fs::File::open(target).with_context(|| format!("open {}", target.display()))?);
  std::io::copy(&mut reader, &mut encoder)?;
  encoder.finish()?;
  Ok(())
}

async fn artifact_size(path: &Path) -> Result<u32> {
  let len = tokio::fs::metadata(path)
    .await
    .with_context(|| format!("stat artifact {}", path.display()))?
    .len();
  u32::try_from(len).map_err(|_| anyhow!("artifact larger than 4 GiB; refusing"))
}

async fn hash_file(path: &Path) -> Result<String> {
  let owned = path.to_path_buf();
  tokio::task::spawn_blocking(move || bridgething_delivery::bundle::fetch::sha256_file(&owned))
    .await
    .map_err(|err| anyhow!("hash task join: {err}"))?
    .map_err(|err| anyhow!("sha256 {}: {err}", path.display()))
}
