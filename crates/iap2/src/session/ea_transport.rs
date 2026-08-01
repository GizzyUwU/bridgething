use bytes::{Bytes, BytesMut};
use tokio::{sync::mpsc, task::JoinHandle};

use crate::{frame::LINK_FRAME_OVERHEAD, link::Iap2Command};

pub(crate) const EA_LINK_SESSION_ID: u8 = 3;
const EA_STREAM_ID_PREFIX_LEN: usize = 2;
const LANE_CAPACITY: usize = 16;
const LANE_STARVATION_GUARD: u32 = 8;

type FramedBytes = (u16, Bytes);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EaPriority {
  #[default]
  Normal,
  Bulk,
  Background,
}

#[derive(Debug, thiserror::Error)]
pub enum EaSendError {
  #[error("EA chunker is no longer running for this link")]
  ChannelClosed,
}

#[derive(Debug, Clone)]
pub struct EaStreamSender {
  stream_id: u16,
  normal_tx: mpsc::Sender<FramedBytes>,
  bulk_tx: mpsc::Sender<FramedBytes>,
  background_tx: mpsc::Sender<FramedBytes>,
}

impl EaStreamSender {
  pub fn stream_id(&self) -> u16 {
    self.stream_id
  }

  pub async fn send(&self, priority: EaPriority, frame: Bytes) -> std::result::Result<(), EaSendError> {
    let lane = match priority {
      EaPriority::Normal => &self.normal_tx,
      EaPriority::Bulk => &self.bulk_tx,
      EaPriority::Background => &self.background_tx,
    };
    lane
      .send((self.stream_id, frame))
      .await
      .map_err(|_| EaSendError::ChannelClosed)
  }
}

pub(crate) struct EaChunker {
  normal_tx: mpsc::Sender<FramedBytes>,
  bulk_tx: mpsc::Sender<FramedBytes>,
  background_tx: mpsc::Sender<FramedBytes>,
  _handle: JoinHandle<()>,
}

impl EaChunker {
  pub(crate) fn new(link_command_tx: mpsc::Sender<Iap2Command>, peer_max_len: u16) -> Self {
    let (normal_tx, normal_rx) = mpsc::channel(LANE_CAPACITY);
    let (bulk_tx, bulk_rx) = mpsc::channel(LANE_CAPACITY);
    let (background_tx, background_rx) = mpsc::channel(LANE_CAPACITY);
    let max_chunk = max_chunk_payload(peer_max_len);
    let _handle = tokio::spawn(chunker_task(
      normal_rx,
      bulk_rx,
      background_rx,
      link_command_tx,
      max_chunk,
    ));
    Self {
      normal_tx,
      bulk_tx,
      background_tx,
      _handle,
    }
  }

  pub(crate) fn sender(&self, stream_id: u16) -> EaStreamSender {
    EaStreamSender {
      stream_id,
      normal_tx: self.normal_tx.clone(),
      bulk_tx: self.bulk_tx.clone(),
      background_tx: self.background_tx.clone(),
    }
  }
}

pub(crate) fn split_stream_frame(payload: &Bytes) -> Option<(u16, Bytes)> {
  if payload.len() < 2 {
    return None;
  }
  let stream_id = u16::from_be_bytes([payload[0], payload[1]]);
  Some((stream_id, payload.slice(2..)))
}

const fn max_chunk_payload(peer_max_len: u16) -> usize {
  let overhead = LINK_FRAME_OVERHEAD + EA_STREAM_ID_PREFIX_LEN;
  let total = peer_max_len as usize;
  if total <= overhead { 1 } else { total - overhead }
}

async fn chunker_task(
  normal_rx: mpsc::Receiver<FramedBytes>,
  bulk_rx: mpsc::Receiver<FramedBytes>,
  background_rx: mpsc::Receiver<FramedBytes>,
  link_tx: mpsc::Sender<Iap2Command>,
  max_chunk_payload: usize,
) {
  struct LaneBuf {
    rx: mpsc::Receiver<FramedBytes>,
    queue: std::collections::VecDeque<FramedBytes>,
  }

  impl LaneBuf {
    fn drain_ready(&mut self) {
      while let Ok(frame) = self.rx.try_recv() {
        self.queue.push_back(frame);
      }
    }

    fn next_packet(&mut self, max_payload: usize) -> Option<(u16, Bytes)> {
      let stream_id = self.queue.front().map(|(id, _)| *id)?;
      let mut out = BytesMut::with_capacity(max_payload.min(64 * 1024));
      while out.len() < max_payload {
        let Some((id, bytes)) = self.queue.front_mut() else {
          break;
        };
        if *id != stream_id {
          break;
        }
        let take = (max_payload - out.len()).min(bytes.len());
        out.extend_from_slice(&bytes.split_to(take));
        if bytes.is_empty() {
          self.queue.pop_front();
        }
      }
      Some((stream_id, out.freeze()))
    }
  }

  fn pick_lane(lanes: &[LaneBuf; 3], skipped: &[u32; 3]) -> Option<usize> {
    lanes
      .iter()
      .enumerate()
      .find(|(i, l)| !l.queue.is_empty() && skipped[*i] >= LANE_STARVATION_GUARD)
      .map(|(i, _)| i)
      .or_else(|| lanes.iter().position(|l| !l.queue.is_empty()))
  }

  let lane = |rx| LaneBuf {
    rx,
    queue: std::collections::VecDeque::new(),
  };
  let mut lanes = [lane(normal_rx), lane(bulk_rx), lane(background_rx)];
  let mut skipped = [0u32; 3];

  loop {
    for lane in &mut lanes {
      lane.drain_ready();
    }

    if let Some(idx) = pick_lane(&lanes, &skipped) {
      for (i, lane) in lanes.iter().enumerate() {
        if lane.queue.is_empty() {
          skipped[i] = 0;
        } else if i != idx {
          skipped[i] += 1;
        }
      }
      skipped[idx] = 0;

      let Some((stream_id, payload)) = lanes[idx].next_packet(max_chunk_payload) else {
        continue;
      };
      if !send_packet(&link_tx, stream_id, payload).await {
        return;
      }
      continue;
    }

    let [normal, bulk, background] = &mut lanes;
    tokio::select! {
      biased;
      Some(f) = normal.rx.recv() => normal.queue.push_back(f),
      Some(f) = bulk.rx.recv() => bulk.queue.push_back(f),
      Some(f) = background.rx.recv() => background.queue.push_back(f),
      else => return,
    };
  }
}

async fn send_packet(link_tx: &mpsc::Sender<Iap2Command>, stream_id: u16, payload: Bytes) -> bool {
  let mut wire = BytesMut::with_capacity(2 + payload.len());
  wire.extend_from_slice(&stream_id.to_be_bytes());
  wire.extend_from_slice(&payload);
  link_tx
    .send(Iap2Command::Send {
      session_id: EA_LINK_SESSION_ID,
      payload: wire.freeze(),
    })
    .await
    .is_ok()
}

#[cfg(test)]
mod tests {
  use super::*;

  fn drain_chunks(rx: &mut mpsc::Receiver<Iap2Command>) -> Vec<Bytes> {
    let mut out = Vec::new();
    while let Ok(cmd) = rx.try_recv() {
      if let Iap2Command::Send { session_id, payload } = cmd {
        assert_eq!(session_id, EA_LINK_SESSION_ID);
        out.push(payload);
      }
    }
    out
  }

  fn assert_chunk(payload: &Bytes, expected_stream: u16, expected_data: &[u8]) {
    assert!(payload.len() >= 2);
    let stream = u16::from_be_bytes([payload[0], payload[1]]);
    assert_eq!(stream, expected_stream);
    assert_eq!(&payload[2..], expected_data);
  }

  #[tokio::test]
  async fn chunker_splits_large_frame() {
    let (link_tx, mut link_rx) = mpsc::channel(64);
    let (n_tx, n_rx) = mpsc::channel(8);
    let (_b_tx, b_rx) = mpsc::channel(8);
    let (_g_tx, g_rx) = mpsc::channel(8);
    tokio::spawn(chunker_task(n_rx, b_rx, g_rx, link_tx, 4));

    let payload = Bytes::from_static(&[1, 2, 3, 4, 5, 6, 7, 8, 9]);
    n_tx.send((0x0100, payload)).await.unwrap();
    drop(n_tx);

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let chunks = drain_chunks(&mut link_rx);
    assert_eq!(chunks.len(), 3, "9 bytes / 4 chunk size = 3 chunks");
    assert_chunk(&chunks[0], 0x0100, &[1, 2, 3, 4]);
    assert_chunk(&chunks[1], 0x0100, &[5, 6, 7, 8]);
    assert_chunk(&chunks[2], 0x0100, &[9]);
  }

  #[tokio::test]
  async fn chunker_normal_preempts_bulk_at_chunk_boundary() {
    let (link_tx, mut link_rx) = mpsc::channel(64);
    let (n_tx, n_rx) = mpsc::channel(8);
    let (b_tx, b_rx) = mpsc::channel(8);
    let (_g_tx, g_rx) = mpsc::channel(8);
    tokio::spawn(chunker_task(n_rx, b_rx, g_rx, link_tx, 4));

    b_tx
      .send((
        0x0200,
        Bytes::from_static(&[0xB0, 0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7]),
      ))
      .await
      .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    n_tx.send((0x0100, Bytes::from_static(&[0xA0, 0xA1]))).await.unwrap();

    drop(n_tx);
    drop(b_tx);
    tokio::time::sleep(std::time::Duration::from_millis(40)).await;

    let chunks = drain_chunks(&mut link_rx);
    let stream_seq: Vec<u16> = chunks.iter().map(|p| u16::from_be_bytes([p[0], p[1]])).collect();
    assert!(
      stream_seq.windows(2).any(|w| w[0] == 0x0200 && w[1] == 0x0100),
      "Normal stream chunk lands between Bulk chunks (got {:?})",
      stream_seq
    );
    let collected_bulk: Vec<u8> = chunks
      .iter()
      .filter(|p| u16::from_be_bytes([p[0], p[1]]) == 0x0200)
      .flat_map(|p| p[2..].to_vec())
      .collect();
    assert_eq!(collected_bulk, vec![0xB0, 0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7]);
  }

  #[tokio::test]
  async fn chunker_coalesces_same_stream_frames_into_full_packets() {
    let (link_tx, mut link_rx) = mpsc::channel(64);
    let (n_tx, n_rx) = mpsc::channel(8);
    let (_b_tx, b_rx) = mpsc::channel(8);
    let (_g_tx, g_rx) = mpsc::channel(8);
    tokio::spawn(chunker_task(n_rx, b_rx, g_rx, link_tx, 4));

    n_tx.send((0x0100, Bytes::from_static(&[1, 2, 3]))).await.unwrap();
    n_tx.send((0x0100, Bytes::from_static(&[4, 5, 6]))).await.unwrap();
    drop(n_tx);
    tokio::time::sleep(std::time::Duration::from_millis(40)).await;

    let chunks = drain_chunks(&mut link_rx);
    let sizes: Vec<usize> = chunks.iter().map(|p| p.len() - 2).collect();
    assert_eq!(sizes, vec![4, 2], "frames must coalesce to fill the link budget");
    let collected: Vec<u8> = chunks.iter().flat_map(|p| p[2..].to_vec()).collect();
    assert_eq!(collected, vec![1, 2, 3, 4, 5, 6]);
  }

  #[tokio::test]
  async fn chunker_never_mixes_streams_in_one_packet() {
    let (link_tx, mut link_rx) = mpsc::channel(64);
    let (n_tx, n_rx) = mpsc::channel(8);
    let (_b_tx, b_rx) = mpsc::channel(8);
    let (_g_tx, g_rx) = mpsc::channel(8);
    tokio::spawn(chunker_task(n_rx, b_rx, g_rx, link_tx, 8));

    n_tx.send((0x0100, Bytes::from_static(&[1, 2, 3]))).await.unwrap();
    n_tx.send((0x0200, Bytes::from_static(&[4, 5, 6]))).await.unwrap();
    drop(n_tx);
    tokio::time::sleep(std::time::Duration::from_millis(40)).await;

    let chunks = drain_chunks(&mut link_rx);
    assert_eq!(
      chunks.len(),
      2,
      "a packet carries one stream-id prefix; streams must not mix"
    );
    assert_eq!(u16::from_be_bytes([chunks[0][0], chunks[0][1]]), 0x0100);
    assert_eq!(&chunks[0][2..], &[1, 2, 3]);
    assert_eq!(u16::from_be_bytes([chunks[1][0], chunks[1][1]]), 0x0200);
    assert_eq!(&chunks[1][2..], &[4, 5, 6]);
  }

  #[tokio::test]
  async fn chunker_bulk_preempts_background_at_chunk_boundary() {
    let (link_tx, mut link_rx) = mpsc::channel(64);
    let (_n_tx, n_rx) = mpsc::channel(8);
    let (b_tx, b_rx) = mpsc::channel(8);
    let (g_tx, g_rx) = mpsc::channel(8);
    tokio::spawn(chunker_task(n_rx, b_rx, g_rx, link_tx, 4));

    g_tx
      .send((
        0x0300,
        Bytes::from_static(&[0xC0, 0xC1, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7]),
      ))
      .await
      .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    b_tx.send((0x0200, Bytes::from_static(&[0xB0, 0xB1]))).await.unwrap();

    drop(b_tx);
    drop(g_tx);
    tokio::time::sleep(std::time::Duration::from_millis(40)).await;

    let chunks = drain_chunks(&mut link_rx);
    let stream_seq: Vec<u16> = chunks.iter().map(|p| u16::from_be_bytes([p[0], p[1]])).collect();
    assert!(
      stream_seq.windows(2).any(|w| w[0] == 0x0300 && w[1] == 0x0200),
      "Bulk stream chunk lands between Background chunks (got {:?})",
      stream_seq
    );
    let collected_background: Vec<u8> = chunks
      .iter()
      .filter(|p| u16::from_be_bytes([p[0], p[1]]) == 0x0300)
      .flat_map(|p| p[2..].to_vec())
      .collect();
    assert_eq!(
      collected_background,
      vec![0xC0, 0xC1, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7]
    );
  }

  #[tokio::test]
  async fn a_saturated_normal_lane_cannot_hold_the_link_forever() {
    let (link_tx, mut link_rx) = mpsc::channel(4096);
    let (n_tx, n_rx) = mpsc::channel(512);
    let (b_tx, b_rx) = mpsc::channel(8);
    let (_g_tx, g_rx) = mpsc::channel(8);
    tokio::spawn(chunker_task(n_rx, b_rx, g_rx, link_tx, 4));

    let feeder = tokio::spawn(async move {
      for _ in 0..2000 {
        if n_tx.send((0x0100, Bytes::from_static(&[0xA0; 4]))).await.is_err() {
          return;
        }
      }
    });

    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    let _ = drain_chunks(&mut link_rx);
    b_tx.send((0x0200, Bytes::from_static(&[0xB0; 4]))).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    let chunks = drain_chunks(&mut link_rx);
    feeder.abort();

    let bulk_at = chunks
      .iter()
      .position(|p| u16::from_be_bytes([p[0], p[1]]) == 0x0200)
      .expect("bulk must get a turn while normal is saturated");
    assert!(
      bulk_at <= LANE_STARVATION_GUARD as usize + 4,
      "bulk waited {bulk_at} packets after becoming available, past the starvation guard"
    );
  }

  #[test]
  fn split_stream_frame_extracts_prefix() {
    let mut wire = BytesMut::new();
    wire.extend_from_slice(&0x0100u16.to_be_bytes());
    wire.extend_from_slice(&[0xDE, 0xAD]);
    let (id, rest) = split_stream_frame(&wire.freeze()).unwrap();
    assert_eq!(id, 0x0100);
    assert_eq!(&rest[..], &[0xDE, 0xAD]);
  }

  #[test]
  fn split_stream_frame_rejects_short() {
    assert!(split_stream_frame(&Bytes::from_static(&[0x01])).is_none());
  }

  #[tokio::test]
  async fn prefixed_packet_never_exceeds_link_payload_budget() {
    let peer_max_len: u16 = 2048;
    let link_budget = peer_max_len as usize - LINK_FRAME_OVERHEAD;
    let max_chunk = max_chunk_payload(peer_max_len);

    let (link_tx, mut link_rx) = mpsc::channel(256);
    let (n_tx, n_rx) = mpsc::channel(8);
    let (_b_tx, b_rx) = mpsc::channel(8);
    let (_g_tx, g_rx) = mpsc::channel(8);
    tokio::spawn(chunker_task(n_rx, b_rx, g_rx, link_tx, max_chunk));

    n_tx
      .send((0x0100, Bytes::from(vec![0xAB; max_chunk * 3 + 7])))
      .await
      .unwrap();
    drop(n_tx);
    tokio::time::sleep(std::time::Duration::from_millis(40)).await;

    let chunks = drain_chunks(&mut link_rx);
    assert!(!chunks.is_empty());
    for c in &chunks {
      assert!(
        c.len() <= link_budget,
        "wire packet {} bytes exceeds link payload budget {}",
        c.len(),
        link_budget
      );
    }
  }
}
