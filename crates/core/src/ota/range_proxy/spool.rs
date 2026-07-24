use std::path::Path;

use tokio::{
  fs::{File, OpenOptions},
  io::{AsyncReadExt, AsyncWriteExt},
  sync::watch,
};
use tokio_util::bytes::Bytes;

#[derive(Debug, Clone)]
enum SpoolState {
  Open { committed: u64 },
  Finished { committed: u64 },
  Failed { reason: String },
}

#[derive(Debug)]
pub(super) struct SpoolWriter {
  file: File,
  committed: u64,
  state_tx: watch::Sender<SpoolState>,
}

#[derive(Debug)]
pub(super) struct SpoolReader {
  file: File,
  pos: u64,
  state_rx: watch::Receiver<SpoolState>,
}

pub(super) async fn create(dir: &Path, name: &str) -> std::io::Result<(SpoolWriter, SpoolReader)> {
  tokio::fs::create_dir_all(dir).await?;
  let path = dir.join(name);
  let write = OpenOptions::new()
    .create(true)
    .truncate(true)
    .write(true)
    .open(&path)
    .await?;
  let read = File::open(&path).await?;
  tokio::fs::remove_file(&path).await?;
  let (state_tx, state_rx) = watch::channel(SpoolState::Open { committed: 0 });
  Ok((
    SpoolWriter {
      file: write,
      committed: 0,
      state_tx,
    },
    SpoolReader {
      file: read,
      pos: 0,
      state_rx,
    },
  ))
}

impl SpoolWriter {
  pub(super) async fn append(&mut self, bytes: &[u8]) -> std::io::Result<u64> {
    self.file.write_all(bytes).await?;
    self.file.flush().await?;
    self.committed += bytes.len() as u64;
    let _ = self.state_tx.send(SpoolState::Open {
      committed: self.committed,
    });
    Ok(self.committed)
  }

  pub(super) fn finish(self) {
    let _ = self.state_tx.send(SpoolState::Finished {
      committed: self.committed,
    });
  }

  pub(super) fn fail(self, reason: impl Into<String>) {
    let _ = self.state_tx.send(SpoolState::Failed { reason: reason.into() });
  }
}

impl SpoolReader {
  pub(super) async fn next(&mut self, max: usize) -> std::io::Result<Bytes> {
    loop {
      let state = self.state_rx.borrow_and_update().clone();
      let (committed, finished) = match &state {
        SpoolState::Open { committed } => (*committed, false),
        SpoolState::Finished { committed } => (*committed, true),
        SpoolState::Failed { reason } => return Err(std::io::Error::other(reason.clone())),
      };
      if self.pos < committed {
        let take = max.min((committed - self.pos) as usize).max(1);
        let mut buf = vec![0u8; take];
        self.file.read_exact(&mut buf).await?;
        self.pos += take as u64;
        return Ok(Bytes::from(buf));
      }
      if finished {
        return Err(std::io::Error::other("spool finished before requested bytes"));
      }
      if self.state_rx.changed().await.is_err() {
        return Err(std::io::Error::other("spool writer dropped without terminal state"));
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use std::{path::PathBuf, time::Duration};

  use super::*;

  fn temp_dir() -> PathBuf {
    std::env::temp_dir().join(format!("bridgething-spool-test-{}", uuid::Uuid::now_v7()))
  }

  #[tokio::test]
  async fn reader_tails_writer_across_appends() {
    let dir = temp_dir();
    let (mut w, mut r) = create(&dir, "t1").await.unwrap();
    w.append(b"hello ").await.unwrap();
    assert_eq!(&r.next(1024).await.unwrap()[..], b"hello ");
    let bg = tokio::spawn(async move {
      tokio::time::sleep(Duration::from_millis(50)).await;
      w.append(b"world").await.unwrap();
      w.finish();
    });
    assert_eq!(&r.next(1024).await.unwrap()[..], b"world");
    bg.await.unwrap();
  }

  #[tokio::test]
  async fn reader_respects_max_and_position() {
    let dir = temp_dir();
    let (mut w, mut r) = create(&dir, "t2").await.unwrap();
    w.append(b"abcdef").await.unwrap();
    assert_eq!(&r.next(2).await.unwrap()[..], b"ab");
    assert_eq!(&r.next(2).await.unwrap()[..], b"cd");
    assert_eq!(&r.next(100).await.unwrap()[..], b"ef");
  }

  #[tokio::test]
  async fn fail_propagates_to_pending_reader() {
    let dir = temp_dir();
    let (w, mut r) = create(&dir, "t3").await.unwrap();
    let bg = tokio::spawn(async move {
      tokio::time::sleep(Duration::from_millis(50)).await;
      w.fail("companion gave up");
    });
    let err = r.next(16).await.unwrap_err();
    assert!(err.to_string().contains("companion gave up"));
    bg.await.unwrap();
  }

  #[tokio::test]
  async fn finish_short_of_read_position_errors() {
    let dir = temp_dir();
    let (mut w, mut r) = create(&dir, "t4").await.unwrap();
    w.append(b"xy").await.unwrap();
    w.finish();
    assert_eq!(&r.next(2).await.unwrap()[..], b"xy");
    assert!(r.next(1).await.is_err());
  }

  #[tokio::test]
  async fn dropped_writer_without_terminal_errors() {
    let dir = temp_dir();
    let (w, mut r) = create(&dir, "t5").await.unwrap();
    drop(w);
    assert!(r.next(1).await.is_err());
  }

  #[tokio::test]
  async fn spool_file_is_unlinked_at_creation() {
    let dir = temp_dir();
    let (mut w, mut r) = create(&dir, "t6").await.unwrap();
    assert!(!dir.join("t6").exists());
    w.append(b"still works").await.unwrap();
    assert_eq!(&r.next(1024).await.unwrap()[..], b"still works");
  }
}
