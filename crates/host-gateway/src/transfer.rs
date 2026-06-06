//! Fragment-stream helpers for the generic transfer surface: pump a
//! file (or a set of file ranges) to the daemon as `TransferFragment`
//! events without ever holding the payload in memory.

use std::path::Path;

use anyhow::{Result, anyhow};
use libbridgething::{
  Priority,
  gateway::{GatewayToBridgeMsg, GatewayToBridgeMsgData, GatewayToBridgeTransferMsg, TransferFragment},
  wire::MsgMeta,
};
use tokio::{
  io::{AsyncReadExt, AsyncSeekExt},
  sync::mpsc,
};

use crate::conn::OutboundFrame;

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

/// Stream a file's bytes as fragments with file-absolute offsets,
/// starting at `start_offset` (the daemon's resume point).
pub async fn stream_file_fragments(
  out: &mpsc::Sender<OutboundFrame>,
  transfer_id: uuid::Uuid,
  path: &Path,
  start_offset: u64,
  total_size: u64,
  chunk_size: usize,
  priority: Priority,
) -> Result<()> {
  let mut file = tokio::fs::File::open(path).await?;
  if start_offset > 0 {
    file.seek(std::io::SeekFrom::Start(start_offset)).await?;
  }

  let mut buf = vec![0u8; chunk_size];
  let mut offset = start_offset;
  while offset < total_size {
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

/// Stream a set of file ranges as one fragment stream with
/// stream-relative offsets (0..sum of range lengths, ranges
/// concatenated in declaration order).
pub async fn stream_range_fragments(
  out: &mpsc::Sender<OutboundFrame>,
  transfer_id: uuid::Uuid,
  path: &Path,
  ranges: &[(u32, u32)],
  chunk_size: usize,
  priority: Priority,
) -> Result<()> {
  let mut file = tokio::fs::File::open(path).await?;
  let mut buf = vec![0u8; chunk_size];
  let mut stream_offset: u32 = 0;
  for (start, length) in ranges {
    file.seek(std::io::SeekFrom::Start(*start as u64)).await?;
    let mut produced: u32 = 0;
    while produced < *length {
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
