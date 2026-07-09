//! Fragment-stream helpers for the generic transfer surface: pump a
//! file (or a set of file ranges) to the daemon as `TransferFragment`
//! events without ever holding the payload in memory.

use std::{
  collections::HashMap,
  path::Path,
  sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
  },
  time::Duration,
};

use anyhow::{Result, anyhow};
use libbridgething::{
  Priority,
  gateway::{GatewayToBridgeMsg, GatewayToBridgeMsgData, GatewayToBridgeTransferMsg, TransferFragment},
  wire::MsgMeta,
};
use tokio::{
  io::{AsyncReadExt, AsyncSeekExt},
  sync::{Notify, mpsc},
};
use uuid::Uuid;

use crate::conn::OutboundFrame;

const OTA_WINDOW_BYTES: u64 = 512 * 1024;
const OTA_ACK_TIMEOUT: Duration = Duration::from_secs(30);

pub struct AckWindow {
  acked: AtomicU64,
  progress: Notify,
}

impl AckWindow {
  fn new(baseline: u64) -> Arc<Self> {
    Arc::new(Self {
      acked: AtomicU64::new(baseline),
      progress: Notify::new(),
    })
  }

  pub fn note(&self, received: u64) {
    let mut cur = self.acked.load(Ordering::Acquire);
    while received > cur {
      match self
        .acked
        .compare_exchange_weak(cur, received, Ordering::AcqRel, Ordering::Acquire)
      {
        Ok(_) => {
          self.progress.notify_one();
          return;
        }
        Err(actual) => cur = actual,
      }
    }
  }

  fn acked(&self) -> u64 {
    self.acked.load(Ordering::Acquire)
  }

  async fn wait_for_room(&self, offset: u64) -> bool {
    loop {
      if offset < self.acked().saturating_add(OTA_WINDOW_BYTES) {
        return true;
      }
      let prior = self.acked();
      let notified = self.progress.notified();
      if offset < self.acked().saturating_add(OTA_WINDOW_BYTES) {
        return true;
      }
      match tokio::time::timeout(OTA_ACK_TIMEOUT, notified).await {
        Ok(()) => {}
        Err(_) => {
          if self.acked() <= prior {
            return false;
          }
        }
      }
    }
  }
}

#[derive(Clone, Default)]
pub struct AckRegistry {
  windows: Arc<Mutex<HashMap<Uuid, Arc<AckWindow>>>>,
}

impl AckRegistry {
  pub fn register(&self, transfer_id: Uuid, baseline: u64) -> Arc<AckWindow> {
    let window = AckWindow::new(baseline);
    self.windows.lock().unwrap().insert(transfer_id, window.clone());
    window
  }

  pub fn deregister(&self, transfer_id: Uuid) {
    self.windows.lock().unwrap().remove(&transfer_id);
  }

  pub fn note(&self, transfer_id: Uuid, received: u64) {
    if let Some(window) = self.windows.lock().unwrap().get(&transfer_id) {
      window.note(received);
    }
  }
}

fn fragment_frame(transfer_id: uuid::Uuid, offset: u32, bytes: Vec<u8>, priority: Priority) -> OutboundFrame {
  OutboundFrame {
    msg: GatewayToBridgeMsg {
      id: uuid::Uuid::now_v7(),
      meta: MsgMeta::Event,
      data: GatewayToBridgeMsgData::Transfer(GatewayToBridgeTransferMsg::Fragment(TransferFragment {
        transfer_id,
        offset,
        bytes,
      })),
    },
    priority,
  }
}

#[allow(clippy::too_many_arguments)]
pub async fn stream_file_fragments(
  out: &mpsc::Sender<OutboundFrame>,
  transfer_id: uuid::Uuid,
  path: &Path,
  start_offset: u64,
  total_size: u64,
  chunk_size: usize,
  priority: Priority,
  window: &AckWindow,
) -> Result<()> {
  let mut file = tokio::fs::File::open(path).await?;
  if start_offset > 0 {
    file.seek(std::io::SeekFrom::Start(start_offset)).await?;
  }

  let mut buf = vec![0u8; chunk_size];
  let mut offset = start_offset;
  while offset < total_size {
    if !window.wait_for_room(offset).await {
      return Err(anyhow!(
        "transfer stalled: no drain-ack progress at offset {offset}/{total_size}"
      ));
    }
    let n = file.read(&mut buf).await?;
    if n == 0 {
      return Err(anyhow!("unexpected EOF at offset {offset}/{total_size}"));
    }
    let frame = fragment_frame(
      transfer_id,
      u32::try_from(offset).map_err(|_| anyhow!("fragment offset overflow"))?,
      buf[..n].to_vec(),
      priority,
    );
    out
      .send(frame)
      .await
      .map_err(|_| anyhow!("connection writer closed mid-stream at offset {offset}"))?;
    offset += n as u64;
  }
  tracing::info!(offset, "fragment stream complete");
  Ok(())
}

pub async fn stream_range_fragments(
  out: &mpsc::Sender<OutboundFrame>,
  transfer_id: uuid::Uuid,
  path: &Path,
  ranges: &[(u32, u32)],
  chunk_size: usize,
  priority: Priority,
  window: &AckWindow,
) -> Result<()> {
  let mut file = tokio::fs::File::open(path).await?;
  let mut buf = vec![0u8; chunk_size];
  let mut stream_offset: u32 = 0;
  for (start, length) in ranges {
    file.seek(std::io::SeekFrom::Start(*start as u64)).await?;
    let mut produced: u32 = 0;
    while produced < *length {
      if !window.wait_for_room(stream_offset as u64).await {
        return Err(anyhow!(
          "range stream stalled: no drain-ack progress at offset {stream_offset}"
        ));
      }
      let want = (*length - produced) as usize;
      let to_read = want.min(buf.len());
      let n = file.read(&mut buf[..to_read]).await?;
      if n == 0 {
        return Err(anyhow!(
          "unexpected EOF reading range at offset {}",
          *start as u64 + produced as u64,
        ));
      }
      let frame = fragment_frame(transfer_id, stream_offset, buf[..n].to_vec(), priority);
      out
        .send(frame)
        .await
        .map_err(|_| anyhow!("connection writer closed while serving range"))?;
      produced += n as u32;
      stream_offset += n as u32;
    }
  }
  Ok(())
}
