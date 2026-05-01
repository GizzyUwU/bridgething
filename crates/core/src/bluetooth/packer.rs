//! Drains pre-encoded outbound frames from a Normal lane and a Bulk
//! lane, packing as many as fit into a single batch up to
//! `max_batch_bytes`. Normal is drained first every batch and the
//! remaining space is filled opportunistically from Bulk via
//! `try_recv` - we never wait for Bulk, which is what preserves
//! Normal's latency.
//!
//! Frame ordering is FIFO within a lane, with Normal globally
//! preceding Bulk. A frame larger than the batch budget is flushed
//! solo (one frame per batch). Overflow (a frame that doesn't fit
//! after earlier frames consumed the budget) is stashed in its
//! lane's slot and seeded into the next batch in the same Normal-
//! before-Bulk order.
//!
//! Owns the lane channels by value: the packer is the sole reader
//! and the writer task it feeds is the sole writer. Both lanes
//! closing returns `None` from `next_batch`, which the writer task
//! treats as shutdown.

use tokio::sync::mpsc;
use tokio_util::bytes::{Bytes, BytesMut};

pub struct OutboundPacker {
  normal_rx: mpsc::Receiver<Bytes>,
  bulk_rx: mpsc::Receiver<Bytes>,
  max_batch_bytes: usize,
  normal_pending: Option<Bytes>,
  bulk_pending: Option<Bytes>,
}

impl OutboundPacker {
  pub fn new(normal_rx: mpsc::Receiver<Bytes>, bulk_rx: mpsc::Receiver<Bytes>, max_batch_bytes: usize) -> Self {
    assert!(max_batch_bytes > 0, "max_batch_bytes must be > 0");
    Self {
      normal_rx,
      bulk_rx,
      max_batch_bytes,
      normal_pending: None,
      bulk_pending: None,
    }
  }

  pub async fn next_batch(&mut self) -> Option<BytesMut> {
    let mut batch = BytesMut::with_capacity(self.max_batch_bytes);

    let seed = self
      .normal_pending
      .take()
      .or_else(|| self.normal_rx.try_recv().ok())
      .or_else(|| self.bulk_pending.take())
      .or_else(|| self.bulk_rx.try_recv().ok());
    let seed = match seed {
      Some(b) => b,
      None => tokio::select! {
        biased;
        Some(b) = self.normal_rx.recv() => b,
        Some(b) = self.bulk_rx.recv() => b,
        else => return None,
      },
    };
    batch.extend_from_slice(&seed);

    loop {
      let next = self.normal_pending.take().or_else(|| self.normal_rx.try_recv().ok());
      let Some(b) = next else { break };
      if batch.len() + b.len() > self.max_batch_bytes {
        self.normal_pending = Some(b);
        return Some(batch);
      }
      batch.extend_from_slice(&b);
    }

    loop {
      let next = self.bulk_pending.take().or_else(|| self.bulk_rx.try_recv().ok());
      let Some(b) = next else { break };
      if batch.len() + b.len() > self.max_batch_bytes {
        self.bulk_pending = Some(b);
        return Some(batch);
      }
      batch.extend_from_slice(&b);
    }

    Some(batch)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn b(s: &[u8]) -> Bytes {
    Bytes::copy_from_slice(s)
  }

  fn channels(cap: usize) -> ((mpsc::Sender<Bytes>, mpsc::Sender<Bytes>), OutboundPacker) {
    let (n_tx, n_rx) = mpsc::channel(cap);
    let (b_tx, b_rx) = mpsc::channel(cap);
    let packer = OutboundPacker::new(n_rx, b_rx, 1024);
    ((n_tx, b_tx), packer)
  }

  #[tokio::test]
  async fn drains_normal_alone() {
    let ((n, _b), mut p) = channels(8);
    n.send(b(b"hello")).await.unwrap();
    n.send(b(b"world")).await.unwrap();
    drop(n);
    let batch = p.next_batch().await.unwrap();
    assert_eq!(&batch[..], b"helloworld");
  }

  #[tokio::test]
  async fn drains_bulk_alone() {
    let ((_n, b_tx), mut p) = channels(8);
    b_tx.send(b(b"abc")).await.unwrap();
    b_tx.send(b(b"def")).await.unwrap();
    drop(b_tx);
    let batch = p.next_batch().await.unwrap();
    assert_eq!(&batch[..], b"abcdef");
  }

  #[tokio::test]
  async fn normal_drains_before_bulk_in_one_batch() {
    let ((n, b_tx), mut p) = channels(8);
    b_tx.send(b(b"BULK1")).await.unwrap();
    b_tx.send(b(b"BULK2")).await.unwrap();
    n.send(b(b"NORM1")).await.unwrap();
    n.send(b(b"NORM2")).await.unwrap();
    drop(n);
    drop(b_tx);
    let batch = p.next_batch().await.unwrap();
    assert_eq!(&batch[..], b"NORM1NORM2BULK1BULK2");
  }

  #[tokio::test]
  async fn frame_overflow_stashes_normal_for_next_batch() {
    let (n_tx, n_rx) = mpsc::channel(8);
    let (_b_tx, b_rx) = mpsc::channel::<Bytes>(8);
    let mut p = OutboundPacker::new(n_rx, b_rx, 6);

    n_tx.send(b(b"abcd")).await.unwrap();
    n_tx.send(b(b"efgh")).await.unwrap();

    let batch1 = p.next_batch().await.unwrap();
    assert_eq!(&batch1[..], b"abcd", "second normal frame did not fit, stashed");
    drop(n_tx);
    let batch2 = p.next_batch().await.unwrap();
    assert_eq!(&batch2[..], b"efgh", "stashed normal frame seeded next batch");
  }

  #[tokio::test]
  async fn frame_overflow_stashes_bulk_for_next_batch() {
    let (_n_tx, n_rx) = mpsc::channel::<Bytes>(8);
    let (b_tx, b_rx) = mpsc::channel(8);
    let mut p = OutboundPacker::new(n_rx, b_rx, 6);

    b_tx.send(b(b"abcd")).await.unwrap();
    b_tx.send(b(b"efgh")).await.unwrap();

    let batch1 = p.next_batch().await.unwrap();
    assert_eq!(&batch1[..], b"abcd");
    drop(b_tx);
    let batch2 = p.next_batch().await.unwrap();
    assert_eq!(&batch2[..], b"efgh");
  }

  #[tokio::test]
  async fn oversized_single_frame_ships_solo() {
    let (n_tx, n_rx) = mpsc::channel(8);
    let (_b_tx, b_rx) = mpsc::channel::<Bytes>(8);
    let mut p = OutboundPacker::new(n_rx, b_rx, 4);

    n_tx.send(b(b"oversized")).await.unwrap();
    drop(n_tx);
    let batch = p.next_batch().await.unwrap();
    assert_eq!(&batch[..], b"oversized", "lone oversized frame ships solo");
  }

  #[tokio::test]
  async fn returns_none_when_both_lanes_close() {
    let ((n, b_tx), mut p) = channels(1);
    drop(n);
    drop(b_tx);
    assert!(p.next_batch().await.is_none(), "both lanes closed = shutdown");
  }

  #[tokio::test]
  async fn blocks_until_a_frame_arrives() {
    let ((n, _b_tx), mut p) = channels(1);
    let send_after = tokio::spawn(async move {
      tokio::time::sleep(std::time::Duration::from_millis(50)).await;
      n.send(b(b"late")).await.unwrap();
      n
    });
    let batch = p.next_batch().await.unwrap();
    assert_eq!(&batch[..], b"late");
    let _n = send_after.await.unwrap();
  }

  #[tokio::test]
  async fn normal_lane_remains_responsive_when_bulk_closes() {
    let ((n, b_tx), mut p) = channels(8);
    drop(b_tx);
    n.send(b(b"alive")).await.unwrap();
    drop(n);
    let batch = p.next_batch().await.unwrap();
    assert_eq!(&batch[..], b"alive");
  }

  #[tokio::test]
  async fn stashed_bulk_seeds_next_batch_only_when_normal_dry() {
    let (n_tx, n_rx) = mpsc::channel(8);
    let (b_tx, b_rx) = mpsc::channel(8);
    let mut p = OutboundPacker::new(n_rx, b_rx, 6);

    n_tx.send(b(b"NN")).await.unwrap();
    b_tx.send(b(b"BB")).await.unwrap();
    b_tx.send(b(b"BBBB")).await.unwrap();
    drop(b_tx);

    // Batch 1: NN + BB fit (4 bytes), BBBB stashed in bulk_pending.
    let batch1 = p.next_batch().await.unwrap();
    assert_eq!(&batch1[..], b"NNBB");

    // Now queue more normal traffic. The Normal channel is drained
    // before the stashed bulk gets a turn.
    n_tx.send(b(b"XX")).await.unwrap();
    drop(n_tx);

    let batch2 = p.next_batch().await.unwrap();
    assert_eq!(
      &batch2[..],
      b"XXBBBB",
      "Normal arrives ahead of stashed Bulk in next batch"
    );
  }
}
