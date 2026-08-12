use std::{collections::VecDeque, sync::Arc};

use bytes::{Bytes, BytesMut};
use libbridgething::Priority;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc};

const LANE_RESERVE: [usize; 3] = [70, 20, 10];

const LANES: [Priority; 3] = [Priority::Normal, Priority::Bulk, Priority::Background];

const fn lane_index(priority: Priority) -> usize {
  match priority {
    Priority::Normal => 0,
    Priority::Bulk => 1,
    Priority::Background => 2,
  }
}

fn allowance(max: usize, share: usize) -> usize {
  (max.saturating_mul(share) / 100).max(1)
}

pub trait LaneItem {
  fn len(&self) -> usize;
  fn is_empty(&self) -> bool {
    self.len() == 0
  }
}

impl LaneItem for Bytes {
  fn len(&self) -> usize {
    Bytes::len(self)
  }
}

struct Lane<T> {
  queue: VecDeque<T>,
  bytes: usize,
  credit: usize,
}

impl<T: LaneItem> Lane<T> {
  fn new() -> Self {
    Self {
      queue: VecDeque::new(),
      bytes: 0,
      credit: 0,
    }
  }

  fn head_len(&self) -> Option<usize> {
    self.queue.front().map(LaneItem::len)
  }

  fn push(&mut self, item: T) {
    self.bytes += item.len();
    self.queue.push_back(item);
  }

  fn pop(&mut self) -> Option<T> {
    let item = self.queue.pop_front()?;
    self.bytes -= item.len();
    Some(item)
  }

  fn fill(&mut self, batch: &mut Batch<T>, ceiling: usize) {
    while let Some(head) = self.queue.front() {
      let next = batch.bytes + head.len();
      if next > ceiling {
        return;
      }
      let item = self.pop().expect("head peeked");
      batch.bytes = next;
      batch.items.push(item);
    }
  }
}

pub struct Batch<T> {
  pub items: Vec<T>,
  pub bytes: usize,
}

impl Batch<Bytes> {
  pub fn into_bytes(mut self) -> Bytes {
    if self.items.len() == 1 {
      return self.items.pop().expect("one item");
    }
    let mut buf = BytesMut::with_capacity(self.bytes);
    for item in &self.items {
      buf.extend_from_slice(item);
    }
    buf.freeze()
  }
}

pub struct LaneScheduler<T> {
  lanes: [Lane<T>; 3],
  max_batch_bytes: usize,
}

impl<T: LaneItem> LaneScheduler<T> {
  pub fn new(max_batch_bytes: usize) -> Self {
    assert!(max_batch_bytes > 0, "max_batch_bytes must be > 0");
    Self {
      lanes: [Lane::new(), Lane::new(), Lane::new()],
      max_batch_bytes,
    }
  }

  pub fn push(&mut self, priority: Priority, item: T) {
    self.lanes[lane_index(priority)].push(item);
  }

  pub fn queued_bytes(&self) -> usize {
    self.lanes.iter().map(|lane| lane.bytes).sum()
  }

  pub fn lane_bytes(&self, priority: Priority) -> usize {
    self.lanes[lane_index(priority)].bytes
  }

  pub fn is_empty(&self) -> bool {
    self.lanes.iter().all(|lane| lane.queue.is_empty())
  }

  pub fn next_batch(&mut self) -> Option<Batch<T>> {
    let max = self.max_batch_bytes;
    let held: [usize; 3] = std::array::from_fn(|lane| self.lanes[lane].queue.len());
    let first = self.owed().unwrap_or(0);

    let seed = self.lanes[first..].iter_mut().find_map(Lane::pop)?;
    let mut batch = Batch {
      bytes: seed.len(),
      items: vec![seed],
    };

    for (lane, share) in self.lanes.iter_mut().zip(LANE_RESERVE) {
      let ceiling = batch.bytes.saturating_add(allowance(max, share)).min(max);
      lane.fill(&mut batch, ceiling);
    }

    for lane in &mut self.lanes {
      lane.fill(&mut batch, max);
    }

    self.settle(&held);
    Some(batch)
  }

  fn owed(&self) -> Option<usize> {
    self
      .lanes
      .iter()
      .position(|lane| lane.head_len().is_some_and(|len| lane.credit >= len))
  }

  fn settle(&mut self, held: &[usize; 3]) {
    let max = self.max_batch_bytes;
    for ((lane, share), held) in self.lanes.iter_mut().zip(LANE_RESERVE).zip(held) {
      let served = lane.queue.len() < *held;
      lane.credit = if served || lane.queue.is_empty() {
        0
      } else {
        lane.credit.saturating_add(allowance(max, share))
      };
    }
  }
}

struct Held<T> {
  item: T,
  _permit: OwnedSemaphorePermit,
}

impl<T: LaneItem> LaneItem for Held<T> {
  fn len(&self) -> usize {
    self.item.len()
  }
}

struct LaneTx<T> {
  budget: Arc<Semaphore>,
  tx: mpsc::UnboundedSender<Held<T>>,
}

impl<T> Clone for LaneTx<T> {
  fn clone(&self) -> Self {
    Self {
      budget: self.budget.clone(),
      tx: self.tx.clone(),
    }
  }
}

pub struct OutboundLanes<T> {
  lanes: [LaneTx<T>; 3],
  max_lane_bytes: usize,
}

impl<T> Clone for OutboundLanes<T> {
  fn clone(&self) -> Self {
    Self {
      lanes: self.lanes.clone(),
      max_lane_bytes: self.max_lane_bytes,
    }
  }
}

impl<T> std::fmt::Debug for OutboundLanes<T> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("OutboundLanes")
      .field("max_lane_bytes", &self.max_lane_bytes)
      .finish()
  }
}

pub struct LaneFeed<T> {
  lanes: [mpsc::UnboundedReceiver<Held<T>>; 3],
  sched: LaneScheduler<Held<T>>,
}

pub fn lanes<T: LaneItem>(max_lane_bytes: usize, max_batch_bytes: usize) -> (OutboundLanes<T>, LaneFeed<T>) {
  assert!(
    (1..=u32::MAX as usize).contains(&max_lane_bytes),
    "max_lane_bytes must be in 1..=u32::MAX"
  );
  let (normal_tx, normal) = mpsc::unbounded_channel();
  let (bulk_tx, bulk) = mpsc::unbounded_channel();
  let (background_tx, background) = mpsc::unbounded_channel();
  let budget = || Arc::new(Semaphore::new(max_lane_bytes));
  (
    OutboundLanes {
      lanes: [
        LaneTx {
          budget: budget(),
          tx: normal_tx,
        },
        LaneTx {
          budget: budget(),
          tx: bulk_tx,
        },
        LaneTx {
          budget: budget(),
          tx: background_tx,
        },
      ],
      max_lane_bytes,
    },
    LaneFeed {
      lanes: [normal, bulk, background],
      sched: LaneScheduler::new(max_batch_bytes),
    },
  )
}

impl<T: LaneItem> OutboundLanes<T> {
  pub async fn send(&self, priority: Priority, item: T) -> bool {
    let lane = &self.lanes[lane_index(priority)];
    let need = item.len().clamp(1, self.max_lane_bytes) as u32;
    let Ok(permit) = lane.budget.clone().acquire_many_owned(need).await else {
      return false;
    };
    lane.tx.send(Held { item, _permit: permit }).is_ok()
  }
}

impl<T: LaneItem> LaneFeed<T> {
  pub async fn ready(&mut self) -> bool {
    loop {
      self.take_queued();
      if !self.sched.is_empty() {
        return true;
      }
      let [normal, bulk, background] = &mut self.lanes;
      let (priority, item) = tokio::select! {
        biased;
        Some(item) = normal.recv() => (Priority::Normal, item),
        Some(item) = bulk.recv() => (Priority::Bulk, item),
        Some(item) = background.recv() => (Priority::Background, item),
        else => return false,
      };
      self.sched.push(priority, item);
    }
  }

  pub fn take_batch(&mut self) -> Option<Batch<T>> {
    self.take_queued();
    let batch = self.sched.next_batch()?;
    let bytes = batch.bytes;
    let items = batch.items.into_iter().map(|held| held.item).collect();
    Some(Batch { items, bytes })
  }

  pub async fn next_batch(&mut self) -> Option<Batch<T>> {
    if !self.ready().await {
      return None;
    }
    Some(self.take_batch().expect("a ready lane composes a batch"))
  }

  fn take_queued(&mut self) {
    let Self { lanes, sched } = self;
    for (rx, priority) in lanes.iter_mut().zip(LANES) {
      while let Ok(item) = rx.try_recv() {
        sched.push(priority, item);
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use super::*;

  fn b(s: &[u8]) -> Bytes {
    Bytes::copy_from_slice(s)
  }

  fn sched(max_batch: usize) -> LaneScheduler<Bytes> {
    LaneScheduler::new(max_batch)
  }

  fn drain(p: &mut LaneScheduler<Bytes>) -> Option<Vec<u8>> {
    p.next_batch().map(|batch| batch.into_bytes().to_vec())
  }

  #[test]
  fn drains_normal_alone() {
    let mut p = sched(1024);
    p.push(Priority::Normal, b(b"hello"));
    p.push(Priority::Normal, b(b"world"));
    let batch = drain(&mut p).unwrap();
    assert_eq!(&batch[..], b"helloworld");
  }

  #[test]
  fn drains_bulk_alone() {
    let mut p = sched(1024);
    p.push(Priority::Bulk, b(b"abc"));
    p.push(Priority::Bulk, b(b"def"));
    let batch = drain(&mut p).unwrap();
    assert_eq!(&batch[..], b"abcdef");
  }

  #[test]
  fn drains_background_alone() {
    let mut p = sched(1024);
    p.push(Priority::Background, b(b"xyz"));
    let batch = drain(&mut p).unwrap();
    assert_eq!(&batch[..], b"xyz");
  }

  #[test]
  fn lanes_drain_in_priority_order_in_one_batch() {
    let mut p = sched(1024);
    p.push(Priority::Background, b(b"BACK1"));
    p.push(Priority::Bulk, b(b"BULK1"));
    p.push(Priority::Bulk, b(b"BULK2"));
    p.push(Priority::Normal, b(b"NORM1"));
    p.push(Priority::Normal, b(b"NORM2"));
    let batch = drain(&mut p).unwrap();
    assert_eq!(&batch[..], b"NORM1NORM2BULK1BULK2BACK1");
  }

  #[test]
  fn frame_overflow_stashes_normal_for_next_batch() {
    let mut p = sched(6);
    p.push(Priority::Normal, b(b"abcd"));
    p.push(Priority::Normal, b(b"efgh"));

    let batch1 = drain(&mut p).unwrap();
    assert_eq!(&batch1[..], b"abcd", "second normal frame did not fit, stashed");
    let batch2 = drain(&mut p).unwrap();
    assert_eq!(&batch2[..], b"efgh", "stashed normal frame seeded next batch");
  }

  #[test]
  fn frame_overflow_stashes_bulk_for_next_batch() {
    let mut p = sched(6);
    p.push(Priority::Bulk, b(b"abcd"));
    p.push(Priority::Bulk, b(b"efgh"));

    let batch1 = drain(&mut p).unwrap();
    assert_eq!(&batch1[..], b"abcd");
    let batch2 = drain(&mut p).unwrap();
    assert_eq!(&batch2[..], b"efgh");
  }

  #[test]
  fn oversized_single_frame_ships_solo() {
    let mut p = sched(4);
    p.push(Priority::Normal, b(b"oversized"));
    let batch = drain(&mut p).unwrap();
    assert_eq!(&batch[..], b"oversized", "lone oversized frame ships solo");
  }

  #[test]
  fn a_saturated_normal_lane_cannot_starve_the_lanes_below() {
    let mut p = sched(100);

    for _ in 0..200 {
      p.push(Priority::Normal, b(&[b'N'; 10]));
    }
    p.push(Priority::Bulk, b(&[b'B'; 10]));
    p.push(Priority::Background, b(&[b'G'; 10]));

    let batch = drain(&mut p).unwrap();
    assert!(
      batch.contains(&b'B') && batch.contains(&b'G'),
      "bulk and background must each land a frame in the first batch despite a saturated normal lane, got {:?}",
      String::from_utf8_lossy(&batch)
    );
  }

  fn first_batch_carrying(p: &mut LaneScheduler<Bytes>, marks: &[u8], batches: usize) -> Vec<Option<usize>> {
    let mut seen: Vec<Option<usize>> = vec![None; marks.len()];
    for index in 0..batches {
      let batch = drain(p).expect("a queued frame composes a batch");
      for (mark, at) in marks.iter().zip(seen.iter_mut()) {
        if at.is_none() && batch.contains(mark) {
          *at = Some(index);
        }
      }
    }
    seen
  }

  #[test]
  fn a_lane_head_wider_than_its_share_ships_under_a_saturated_normal_lane() {
    let mut p = sched(4096);

    for _ in 0..500 {
      p.push(Priority::Normal, b(&[b'N'; 100]));
    }
    p.push(Priority::Bulk, b(&[b'B'; 1200]));
    p.push(Priority::Background, b(&[b'G'; 600]));

    let seen = first_batch_carrying(&mut p, b"BG", 8);
    assert_eq!(
      seen,
      vec![Some(2), Some(3)],
      "a 1200-byte bulk head and a 600-byte background head each exceed their 819/409-byte share of a 4096-byte \
       batch, so each banks its share until it can take a seed"
    );
  }

  #[test]
  fn a_lane_head_wider_than_the_whole_batch_ships_under_a_saturated_normal_lane() {
    let mut p = sched(4096);

    for _ in 0..800 {
      p.push(Priority::Normal, b(&[b'N'; 100]));
    }
    p.push(Priority::Bulk, b(&[b'B'; 8192]));

    let seen = first_batch_carrying(&mut p, b"B", 16);
    assert_eq!(
      seen,
      vec![Some(11)],
      "a bulk head wider than the whole batch can only ship as a seed, which it takes once its bank covers it"
    );
  }

  #[test]
  fn a_served_lane_banks_nothing() {
    let mut p = sched(4096);

    for _ in 0..500 {
      p.push(Priority::Normal, b(&[b'N'; 100]));
    }
    for _ in 0..500 {
      p.push(Priority::Bulk, b(&[b'B'; 100]));
    }

    for _ in 0..8 {
      let batch = drain(&mut p).unwrap();
      let bulk = batch.iter().filter(|byte| **byte == b'B').count();
      assert_eq!(
        bulk, 800,
        "bulk fits inside its 819-byte share every batch, so it never seeds ahead of normal"
      );
      assert_eq!(batch[0], b'N', "normal keeps the seed while nothing below it is owed");
    }
  }

  #[test]
  fn a_wide_framed_normal_lane_skips_to_the_lanes_below() {
    let mut p = sched(100);
    p.push(Priority::Normal, b(&[b'N'; 60]));
    p.push(Priority::Normal, b(&[b'N'; 60]));
    p.push(Priority::Bulk, b(&[b'B'; 10]));

    let batch = drain(&mut p).unwrap();
    assert_eq!(
      batch.len(),
      70,
      "the non-fitting second normal frame is skipped, not emission-ending, so bulk still lands"
    );
    assert!(batch.contains(&b'B'));

    let batch = drain(&mut p).unwrap();
    assert_eq!(batch.len(), 60, "the skipped frame stayed at its lane's head");
  }

  #[test]
  fn an_idle_lane_donates_its_reserve_to_the_lane_above() {
    let mut p = sched(100);

    for _ in 0..10 {
      p.push(Priority::Normal, b(&[b'N'; 10]));
    }

    let batch = drain(&mut p).unwrap();
    assert_eq!(
      batch.len(),
      100,
      "with bulk and background dry, normal fills the whole batch rather than stopping at its 70% share"
    );
  }

  #[test]
  fn returns_none_when_nothing_is_queued() {
    let mut p = sched(1024);
    assert!(p.is_empty());
    assert!(p.next_batch().is_none(), "nothing queued = nothing to emit");
  }

  #[test]
  fn a_frame_pushed_after_an_empty_batch_seeds_the_next_one() {
    let mut p = sched(1024);
    assert!(p.next_batch().is_none());
    p.push(Priority::Normal, b(b"late"));
    let batch = drain(&mut p).unwrap();
    assert_eq!(&batch[..], b"late");
  }

  #[test]
  fn higher_lanes_emit_when_the_lower_lanes_are_empty() {
    let mut p = sched(1024);
    p.push(Priority::Normal, b(b"alive"));
    let batch = drain(&mut p).unwrap();
    assert_eq!(&batch[..], b"alive");
    assert!(p.is_empty());
  }

  #[test]
  fn stashed_bulk_seeds_next_batch_only_when_normal_dry() {
    let mut p = sched(6);

    p.push(Priority::Normal, b(b"NN"));
    p.push(Priority::Bulk, b(b"BB"));
    p.push(Priority::Bulk, b(b"BBBB"));

    let batch1 = drain(&mut p).unwrap();
    assert_eq!(&batch1[..], b"NNBB");

    p.push(Priority::Normal, b(b"XX"));

    let batch2 = drain(&mut p).unwrap();
    assert_eq!(
      &batch2[..],
      b"XXBBBB",
      "Normal arrives ahead of stashed Bulk in next batch"
    );
  }

  #[test]
  fn bulk_preempts_stashed_background() {
    let mut p = sched(6);

    p.push(Priority::Background, b(b"GGGG"));
    p.push(Priority::Background, b(b"GGGG"));

    let batch1 = drain(&mut p).unwrap();
    assert_eq!(&batch1[..], b"GGGG");

    p.push(Priority::Bulk, b(b"BB"));

    let batch2 = drain(&mut p).unwrap();
    assert_eq!(&batch2[..], b"BBGGGG", "bulk drains ahead of stashed background");
  }

  #[test]
  fn queued_bytes_tracks_each_lane() {
    let mut p = sched(4);
    p.push(Priority::Normal, b(b"NN"));
    p.push(Priority::Bulk, b(b"BBBB"));
    assert_eq!(p.lane_bytes(Priority::Normal), 2);
    assert_eq!(p.lane_bytes(Priority::Bulk), 4);
    assert_eq!(p.lane_bytes(Priority::Background), 0);
    assert_eq!(p.queued_bytes(), 6);

    drain(&mut p).unwrap();
    assert_eq!(p.queued_bytes(), 4, "the bulk frame did not fit and stayed queued");
  }

  #[tokio::test]
  async fn a_full_lane_parks_its_sender_until_a_batch_leaves() {
    let (tx, mut feed) = lanes::<Bytes>(4, 1024);

    assert!(tx.send(Priority::Bulk, b(b"abcd")).await, "the first item fits");
    let parked = {
      let tx = tx.clone();
      tokio::spawn(async move { tx.send(Priority::Bulk, b(b"efgh")).await })
    };
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(!parked.is_finished(), "a lane at budget parks its sender");

    let batch = feed.take_batch().expect("the first item is queued");
    assert_eq!(&batch.into_bytes()[..], b"abcd");
    assert!(parked.await.expect("sender task"), "the emission returns the budget");

    let batch = feed.next_batch().await.expect("the unparked item");
    assert_eq!(&batch.into_bytes()[..], b"efgh");
  }

  #[tokio::test]
  async fn an_item_wider_than_the_budget_takes_the_lane_alone() {
    let (tx, mut feed) = lanes::<Bytes>(4, 1024);
    assert!(tx.send(Priority::Normal, b(b"oversized")).await);
    let batch = feed.next_batch().await.expect("an oversize item still ships");
    assert_eq!(&batch.into_bytes()[..], b"oversized");
  }

  #[tokio::test]
  async fn a_batch_is_composed_when_it_is_taken_not_when_the_lanes_fill() {
    let (tx, mut feed) = lanes::<Bytes>(1024, 1024);
    tx.send(Priority::Background, b(b"GG")).await;
    tx.send(Priority::Normal, b(b"NN")).await;
    let batch = feed.next_batch().await.expect("both items");
    assert_eq!(&batch.into_bytes()[..], b"NNGG", "priority order, not arrival order");
  }

  #[tokio::test]
  async fn the_feed_ends_once_every_sender_is_gone() {
    let (tx, mut feed) = lanes::<Bytes>(1024, 1024);
    tx.send(Priority::Normal, b(b"last")).await;
    drop(tx);
    assert_eq!(&feed.next_batch().await.expect("queued item").into_bytes()[..], b"last");
    assert!(feed.next_batch().await.is_none(), "closed and drained");
  }
}
