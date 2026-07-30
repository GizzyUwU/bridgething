use std::{path::PathBuf, time::Duration};

use bytes::Bytes;
use tokio::{
  io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
  net::UnixStream,
  sync::mpsc,
  task::JoinHandle,
};

use super::Detection;

const REDIAL: Duration = Duration::from_secs(2);
const BACKLOG: usize = 8;

#[derive(Debug, Clone)]
pub struct WakeWordLink {
  pcm: mpsc::Sender<Bytes>,
}

impl WakeWordLink {
  pub fn spawn(socket: PathBuf) -> (Self, mpsc::Receiver<Detection>, JoinHandle<()>) {
    let (pcm_tx, pcm_rx) = mpsc::channel(BACKLOG);
    let (hit_tx, hit_rx) = mpsc::channel(4);
    let handle = tokio::spawn(run(socket, pcm_rx, hit_tx));
    (Self { pcm: pcm_tx }, hit_rx, handle)
  }

  pub fn offer(&self, pcm: Bytes) {
    if self.pcm.try_send(pcm).is_err() {
      tracing::trace!("wake word is behind; dropping a frame");
    }
  }
}

async fn run(socket: PathBuf, mut pcm: mpsc::Receiver<Bytes>, hits: mpsc::Sender<Detection>) {
  loop {
    let stream = match UnixStream::connect(&socket).await {
      Ok(stream) => stream,
      Err(err) => {
        tracing::debug!("wake word sidecar unavailable at {}: {err}", socket.display());
        while pcm.try_recv().is_ok() {}
        tokio::time::sleep(REDIAL).await;
        continue;
      }
    };
    tracing::info!("wake word sidecar attached");

    if pump(stream, &mut pcm, &hits).await.is_none() {
      break;
    }
    tracing::warn!("wake word sidecar detached; will redial");
    tokio::time::sleep(REDIAL).await;
  }
}

async fn pump(stream: UnixStream, pcm: &mut mpsc::Receiver<Bytes>, hits: &mpsc::Sender<Detection>) -> Option<()> {
  let (read, mut write) = stream.into_split();
  let mut lines = BufReader::new(read).lines();

  loop {
    tokio::select! {
      frame = pcm.recv() => {
        let frame = frame?;
        if write.write_all(&frame).await.is_err() {
          return Some(());
        }
      }
      line = lines.next_line() => {
        match line {
          Ok(Some(line)) => match serde_json::from_str::<Detection>(&line) {
            Ok(hit) => {
              tracing::info!(score = hit.score, "wake word fired");
              if hits.send(hit).await.is_err() {
                return None;
              }
            }
            Err(err) => tracing::warn!("undecodable detection {line:?}: {err}"),
          },
          Ok(None) | Err(_) => return Some(()),
        }
      }
    }
  }
}
