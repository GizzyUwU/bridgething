use tokio::sync::mpsc;
use tokio_util::bytes::{Bytes, BytesMut};

struct Lane {
  rx: mpsc::Receiver<Bytes>,
  pending: Option<Bytes>,
}

impl Lane {
  fn try_next(&mut self) -> Option<Bytes> {
    self.pending.take().or_else(|| self.rx.try_recv().ok())
  }
}

fn fill_lane(batch: &mut BytesMut, lane: &mut Lane, ceiling: usize, max_batch_bytes: usize) -> bool {
  loop {
    let Some(b) = lane.try_next() else { return true };
    if batch.len() + b.len() > ceiling {
      let fits_in_a_full_batch = batch.len() + b.len() <= max_batch_bytes;
      lane.pending = Some(b);
      return fits_in_a_full_batch;
    }
    batch.extend_from_slice(&b);
  }
}

const LANE_RESERVE: [f32; 3] = [0.7, 0.2, 0.1];

pub struct OutboundPacker {
  lanes: [Lane; 3],
  max_batch_bytes: usize,
}

impl OutboundPacker {
  pub fn new(
    normal_rx: mpsc::Receiver<Bytes>,
    bulk_rx: mpsc::Receiver<Bytes>,
    background_rx: mpsc::Receiver<Bytes>,
    max_batch_bytes: usize,
  ) -> Self {
    assert!(max_batch_bytes > 0, "max_batch_bytes must be > 0");
    let lane = |rx| Lane { rx, pending: None };
    Self {
      lanes: [lane(normal_rx), lane(bulk_rx), lane(background_rx)],
      max_batch_bytes,
    }
  }

  pub async fn next_batch(&mut self) -> Option<BytesMut> {
    let mut batch = BytesMut::with_capacity(self.max_batch_bytes);

    let seed = self.lanes.iter_mut().find_map(Lane::try_next);
    let seed = match seed {
      Some(b) => b,
      None => {
        let [normal, bulk, background] = &mut self.lanes;
        tokio::select! {
          biased;
          Some(b) = normal.rx.recv() => b,
          Some(b) = bulk.rx.recv() => b,
          Some(b) = background.rx.recv() => b,
          else => return None,
        }
      }
    };
    batch.extend_from_slice(&seed);

    let max_batch_bytes = self.max_batch_bytes;

    for (lane, share) in self.lanes.iter_mut().zip(LANE_RESERVE) {
      let ceiling = ((max_batch_bytes as f32 * share) as usize).max(1);
      let ceiling = batch.len().saturating_add(ceiling).min(max_batch_bytes);
      if !fill_lane(&mut batch, lane, ceiling, max_batch_bytes) {
        return Some(batch);
      }
    }

    for lane in &mut self.lanes {
      if !fill_lane(&mut batch, lane, max_batch_bytes, max_batch_bytes) {
        return Some(batch);
      }
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

  type LaneTxs = (mpsc::Sender<Bytes>, mpsc::Sender<Bytes>, mpsc::Sender<Bytes>);

  fn channels_sized(cap: usize, max_batch: usize) -> (LaneTxs, OutboundPacker) {
    let (n_tx, n_rx) = mpsc::channel(cap);
    let (b_tx, b_rx) = mpsc::channel(cap);
    let (g_tx, g_rx) = mpsc::channel(cap);
    let packer = OutboundPacker::new(n_rx, b_rx, g_rx, max_batch);
    ((n_tx, b_tx, g_tx), packer)
  }

  fn channels(cap: usize) -> (LaneTxs, OutboundPacker) {
    channels_sized(cap, 1024)
  }

  #[tokio::test]
  async fn drains_normal_alone() {
    let ((n, _b, _g), mut p) = channels(8);
    n.send(b(b"hello")).await.unwrap();
    n.send(b(b"world")).await.unwrap();
    drop(n);
    let batch = p.next_batch().await.unwrap();
    assert_eq!(&batch[..], b"helloworld");
  }

  #[tokio::test]
  async fn drains_bulk_alone() {
    let ((_n, b_tx, _g), mut p) = channels(8);
    b_tx.send(b(b"abc")).await.unwrap();
    b_tx.send(b(b"def")).await.unwrap();
    drop(b_tx);
    let batch = p.next_batch().await.unwrap();
    assert_eq!(&batch[..], b"abcdef");
  }

  #[tokio::test]
  async fn drains_background_alone() {
    let ((_n, _b, g), mut p) = channels(8);
    g.send(b(b"xyz")).await.unwrap();
    drop(g);
    let batch = p.next_batch().await.unwrap();
    assert_eq!(&batch[..], b"xyz");
  }

  #[tokio::test]
  async fn lanes_drain_in_priority_order_in_one_batch() {
    let ((n, b_tx, g), mut p) = channels(8);
    g.send(b(b"BACK1")).await.unwrap();
    b_tx.send(b(b"BULK1")).await.unwrap();
    b_tx.send(b(b"BULK2")).await.unwrap();
    n.send(b(b"NORM1")).await.unwrap();
    n.send(b(b"NORM2")).await.unwrap();
    drop(n);
    drop(b_tx);
    drop(g);
    let batch = p.next_batch().await.unwrap();
    assert_eq!(&batch[..], b"NORM1NORM2BULK1BULK2BACK1");
  }

  #[tokio::test]
  async fn frame_overflow_stashes_normal_for_next_batch() {
    let ((n_tx, _b, _g), mut p) = channels_sized(8, 6);

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
    let ((_n, b_tx, _g), mut p) = channels_sized(8, 6);

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
    let ((n_tx, _b, _g), mut p) = channels_sized(8, 4);

    n_tx.send(b(b"oversized")).await.unwrap();
    drop(n_tx);
    let batch = p.next_batch().await.unwrap();
    assert_eq!(&batch[..], b"oversized", "lone oversized frame ships solo");
  }

  #[tokio::test]
  async fn a_saturated_normal_lane_cannot_starve_the_lanes_below() {
    let ((n_tx, b_tx, g_tx), mut p) = channels_sized(512, 100);

    for _ in 0..200 {
      n_tx.send(b(&[b'N'; 10])).await.unwrap();
    }
    b_tx.send(b(&[b'B'; 10])).await.unwrap();
    g_tx.send(b(&[b'G'; 10])).await.unwrap();

    let batch = p.next_batch().await.unwrap();
    assert!(
      batch.contains(&b'B') && batch.contains(&b'G'),
      "bulk and background must each land a frame in the first batch despite a saturated normal lane, got {:?}",
      String::from_utf8_lossy(&batch)
    );
  }

  #[tokio::test]
  async fn an_idle_lane_donates_its_reserve_to_the_lane_above() {
    let ((n_tx, _b, _g), mut p) = channels_sized(512, 100);

    for _ in 0..10 {
      n_tx.send(b(&[b'N'; 10])).await.unwrap();
    }

    let batch = p.next_batch().await.unwrap();
    assert_eq!(
      batch.len(),
      100,
      "with bulk and background dry, normal fills the whole batch rather than stopping at its 70% share"
    );
  }

  #[tokio::test]
  async fn returns_none_when_all_lanes_close() {
    let ((n, b_tx, g), mut p) = channels(1);
    drop(n);
    drop(b_tx);
    drop(g);
    assert!(p.next_batch().await.is_none(), "all lanes closed = shutdown");
  }

  #[tokio::test]
  async fn blocks_until_a_frame_arrives() {
    let ((n, _b, _g), mut p) = channels(1);
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
  async fn higher_lanes_remain_responsive_when_lower_close() {
    let ((n, b_tx, g), mut p) = channels(8);
    drop(b_tx);
    drop(g);
    n.send(b(b"alive")).await.unwrap();
    drop(n);
    let batch = p.next_batch().await.unwrap();
    assert_eq!(&batch[..], b"alive");
  }

  #[tokio::test]
  async fn stashed_bulk_seeds_next_batch_only_when_normal_dry() {
    let ((n_tx, b_tx, _g), mut p) = channels_sized(8, 6);

    n_tx.send(b(b"NN")).await.unwrap();
    b_tx.send(b(b"BB")).await.unwrap();
    b_tx.send(b(b"BBBB")).await.unwrap();
    drop(b_tx);

    let batch1 = p.next_batch().await.unwrap();
    assert_eq!(&batch1[..], b"NNBB");

    n_tx.send(b(b"XX")).await.unwrap();
    drop(n_tx);

    let batch2 = p.next_batch().await.unwrap();
    assert_eq!(
      &batch2[..],
      b"XXBBBB",
      "Normal arrives ahead of stashed Bulk in next batch"
    );
  }

  #[tokio::test]
  async fn bulk_preempts_stashed_background() {
    let ((_n, b_tx, g), mut p) = channels_sized(8, 6);

    g.send(b(b"GGGG")).await.unwrap();
    g.send(b(b"GGGG")).await.unwrap();

    let batch1 = p.next_batch().await.unwrap();
    assert_eq!(&batch1[..], b"GGGG");

    b_tx.send(b(b"BB")).await.unwrap();
    drop(b_tx);
    drop(g);

    let batch2 = p.next_batch().await.unwrap();
    assert_eq!(&batch2[..], b"BBGGGG", "bulk drains ahead of stashed background");
  }
}
