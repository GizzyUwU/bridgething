use std::{
  io::{self, Write},
  path::{Path, PathBuf},
};

use libbridgething::gateway::{OtaPatch, OtaPatchAlgorithm};
use sha2::{Digest, Sha256};
use tokio::task;

#[derive(Debug, thiserror::Error)]
pub enum PatchError {
  #[error("io error during {step}: {source}")]
  Io {
    step: &'static str,
    #[source]
    source: io::Error,
  },
  #[error("zstd patch apply failed: {0}")]
  Apply(String),
  #[error("reconstructed artifact is {got} bytes, expected {expected}")]
  SizeMismatch { got: u64, expected: u64 },
  #[error("reconstructed artifact sha256 {got} != expected {expected}")]
  HashMismatch { got: String, expected: String },
}

pub async fn apply(source: PathBuf, patch: PathBuf, spec: OtaPatch) -> Result<PathBuf, PatchError> {
  let OtaPatchAlgorithm::ZstdPatchFrom = spec.algorithm;
  let out = patch.with_extension("reconstructed");
  let job_out = out.clone();
  let result = task::spawn_blocking(move || reconstruct(&source, &patch, &job_out))
    .await
    .map_err(|e| PatchError::Apply(format!("apply task join: {e}")))?;

  let (len, sha) = match result {
    Ok(v) => v,
    Err(err) => {
      let _ = tokio::fs::remove_file(&out).await;
      return Err(err);
    }
  };

  if len != spec.result_size as u64 {
    let _ = tokio::fs::remove_file(&out).await;
    return Err(PatchError::SizeMismatch {
      got: len,
      expected: spec.result_size as u64,
    });
  }
  if !sha.eq_ignore_ascii_case(&spec.result_sha256) {
    let _ = tokio::fs::remove_file(&out).await;
    return Err(PatchError::HashMismatch {
      got: sha,
      expected: spec.result_sha256,
    });
  }
  Ok(out)
}

fn reconstruct(source: &Path, patch: &Path, out: &Path) -> Result<(u64, String), PatchError> {
  let src_file = std::fs::File::open(source).map_err(|source| PatchError::Io {
    step: "open source",
    source,
  })?;
  let src_map = unsafe { memmap2::Mmap::map(&src_file) }.map_err(|source| PatchError::Io {
    step: "mmap source",
    source,
  })?;

  let patch_file = std::fs::File::open(patch).map_err(|source| PatchError::Io {
    step: "open patch",
    source,
  })?;
  let mut decoder = zstd::stream::read::Decoder::with_ref_prefix(io::BufReader::new(patch_file), &src_map)
    .map_err(|e| PatchError::Apply(format!("init decoder: {e}")))?;
  decoder
    .window_log_max(31)
    .map_err(|e| PatchError::Apply(format!("window_log_max: {e}")))?;

  let out_file = std::fs::File::create(out).map_err(|source| PatchError::Io {
    step: "create reconstruction",
    source,
  })?;
  let mut sink = HashingWriter {
    inner: io::BufWriter::new(out_file),
    hasher: Sha256::new(),
    len: 0,
  };
  io::copy(&mut decoder, &mut sink).map_err(|e| PatchError::Apply(format!("decode: {e}")))?;
  sink.inner.flush().map_err(|source| PatchError::Io {
    step: "flush reconstruction",
    source,
  })?;
  Ok((sink.len, hex::encode(sink.hasher.finalize())))
}

struct HashingWriter<W: Write> {
  inner: W,
  hasher: Sha256,
  len: u64,
}

impl<W: Write> Write for HashingWriter<W> {
  fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
    let n = self.inner.write(buf)?;
    self.hasher.update(&buf[..n]);
    self.len += n as u64;
    Ok(n)
  }

  fn flush(&mut self) -> io::Result<()> {
    self.inner.flush()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn sha256_hex(b: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b);
    hex::encode(h.finalize())
  }

  fn temp_root() -> PathBuf {
    let p = std::env::temp_dir().join(format!("bridgething-patch-test-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&p).unwrap();
    p
  }

  fn make_patch(old: &[u8], new: &[u8]) -> Vec<u8> {
    let mut encoder = zstd::stream::write::Encoder::with_ref_prefix(Vec::new(), 19, old).unwrap();
    encoder
      .set_parameter(zstd::zstd_safe::CParameter::EnableLongDistanceMatching(true))
      .unwrap();
    encoder
      .set_parameter(zstd::zstd_safe::CParameter::WindowLog(27))
      .unwrap();
    encoder.write_all(new).unwrap();
    encoder.finish().unwrap()
  }

  #[tokio::test]
  async fn apply_reconstructs_and_verifies() {
    let root = temp_root();
    let old: Vec<u8> = (0u32..40_000).map(|i| (i * 7) as u8).collect();
    let mut new = old.clone();
    for i in (0..new.len()).step_by(97) {
      new[i] = new[i].wrapping_add(1);
    }
    new.extend_from_slice(b"tail bytes only in the new build");

    let source = root.join("bridgething.current");
    std::fs::write(&source, &old).unwrap();
    let patch = root.join("delta.zst");
    std::fs::write(&patch, make_patch(&old, &new)).unwrap();

    let spec = OtaPatch {
      algorithm: OtaPatchAlgorithm::ZstdPatchFrom,
      result_sha256: sha256_hex(&new),
      result_size: new.len() as u32,
    };
    let out = apply(source, patch, spec).await.unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), new);
  }

  #[tokio::test]
  async fn wrong_source_fails_hash_and_discards() {
    let root = temp_root();
    let old: Vec<u8> = (0u32..20_000).map(|i| i as u8).collect();
    let new: Vec<u8> = (0u32..20_000).map(|i| (i + 3) as u8).collect();
    let patch_bytes = make_patch(&old, &new);

    let stale: Vec<u8> = (0u32..20_000).map(|i| (i + 99) as u8).collect();
    let source = root.join("bridgething.current");
    std::fs::write(&source, &stale).unwrap();
    let patch = root.join("delta.zst");
    std::fs::write(&patch, &patch_bytes).unwrap();

    let spec = OtaPatch {
      algorithm: OtaPatchAlgorithm::ZstdPatchFrom,
      result_sha256: sha256_hex(&new),
      result_size: new.len() as u32,
    };
    let err = apply(source, patch.clone(), spec).await.unwrap_err();
    assert!(matches!(err, PatchError::HashMismatch { .. }));
    assert!(!patch.with_extension("reconstructed").exists());
  }
}
