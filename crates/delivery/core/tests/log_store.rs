use std::{
  fs,
  path::{Path, PathBuf},
};

use bridgething_delivery::log::store::{Level, Limits, LogStore};
use tempfile::TempDir;

struct Rig {
  root: TempDir,
  store: LogStore,
}

impl Rig {
  fn with(limits: Limits) -> Self {
    Self {
      root: TempDir::new().expect("a temp root"),
      store: LogStore::new(limits),
    }
  }

  fn install(&self) {
    self.store.install(self.root.path());
  }

  fn path(&self) -> &Path {
    self.root.path()
  }

  fn seed(&self, id: &str, segments: &[(usize, &str, bool)]) {
    let dir = self.root.path().join(id);
    fs::create_dir_all(&dir).expect("a seeded launch");
    for (index, body, pinned) in segments {
      let name = format!("{index:04}");
      fs::write(dir.join(format!("{name}.log")), body).expect("a seeded segment");
      if *pinned {
        fs::write(dir.join(format!("{name}.keep")), []).expect("a seeded pin");
      }
    }
  }

  fn live_id(&self) -> String {
    self
      .store
      .archives()
      .into_iter()
      .find(|archive| archive.current)
      .expect("a live launch")
      .id
  }

  fn live_launch(&self) -> PathBuf {
    self.root.path().join(self.live_id())
  }

  fn ids(&self) -> Vec<String> {
    self.store.archives().into_iter().map(|archive| archive.id).collect()
  }

  fn segment_names(&self, dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
      .expect("a launch dir")
      .flatten()
      .map(|entry| entry.file_name().to_string_lossy().into_owned())
      .filter(|name| name.ends_with(".log"))
      .collect();
    names.sort();
    names
  }

  fn export(&self, id: Option<&str>) -> String {
    let target = self.root.path().join(format!("bundle-{}.txt", uuid::Uuid::now_v7()));
    self.store.export_to(&target, id).expect("an export");
    fs::read_to_string(&target).expect("a readable bundle")
  }

  fn noise(&self, lines: usize) {
    for index in 0..lines {
      self
        .store
        .record(Level::Info, "t", &format!("line {index} {}", "x".repeat(60)));
    }
    self.store.flush();
  }
}

fn tiny(segment_bytes: u64, segments_per_launch: usize) -> Limits {
  Limits {
    segment_bytes,
    segments_per_launch,
    ..Limits::default()
  }
}

#[test]
fn record_prefixes_every_line_with_level_and_label() {
  let rig = Rig::with(tiny(1024, 2));
  rig.install();
  rig.store.record(Level::Info, "gateway", "hello");
  rig.store.flush();

  let text = rig.export(None);
  let line = text.lines().find(|line| line.contains("hello")).expect("the record");
  let mut parts = line.splitn(6, ' ');
  let date = parts.next().expect("a date");
  let clock = parts.next().expect("a clock");
  assert_eq!(date.len(), 5, "unexpected line shape: {line}");
  assert_eq!(clock.len(), 12, "unexpected line shape: {line}");
  assert!(parts.next().is_some_and(|pid| pid.parse::<u32>().is_ok()), "{line}");
  assert!(parts.next().is_some_and(|tid| tid.parse::<u64>().is_ok()), "{line}");
  assert_eq!(parts.next(), Some("I"), "unexpected line shape: {line}");
  assert_eq!(parts.next(), Some("gateway: hello"), "unexpected line shape: {line}");
}

#[test]
fn a_multi_line_message_becomes_one_prefixed_line_each() {
  let rig = Rig::with(tiny(1024, 2));
  rig.install();
  rig.store.record(Level::Warn, "gateway", "first\nsecond");
  rig.store.flush();

  let text = rig.export(None);
  assert_eq!(text.matches(" W gateway: ").count(), 2);
  assert!(text.contains(" W gateway: first"));
  assert!(text.contains(" W gateway: second"));
}

#[test]
fn a_record_before_install_is_dropped() {
  let rig = Rig::with(Limits::default());
  rig.store.record(Level::Info, "gateway", "ignored");
  assert_eq!(rig.store.retained_bytes(), 0);
  assert!(rig.store.archives().is_empty());
}

#[test]
fn a_raw_line_carries_the_threadtime_prefix_its_reader_delivered() {
  let rig = Rig::with(tiny(1024, 2));
  rig.install();
  rig.store.write("07-30 12:00:00.000  1  1 W daemon: [player] stalled");
  rig.store.flush();

  let text = rig.export(None);
  assert!(text.contains("07-30 12:00:00.000  1  1 W daemon: [player] stalled"));
}

#[test]
fn a_segment_rolls_over_at_the_byte_cap() {
  let rig = Rig::with(tiny(512, 2));
  rig.install();
  let dir = rig.live_launch();

  for index in 0..40 {
    rig
      .store
      .record(Level::Info, "t", &format!("line {index} {}", "x".repeat(40)));
  }
  rig.store.flush();

  assert!(rig.segment_names(&dir).len() > 1);
}

#[test]
fn the_segment_ring_keeps_only_the_newest_segments() {
  let rig = Rig::with(tiny(256, 2));
  rig.install();
  let dir = rig.live_launch();

  rig.noise(200);

  assert_eq!(rig.segment_names(&dir).len(), 2);
  assert!(rig.export(None).contains("line 199"));
}

#[test]
fn a_burst_larger_than_a_segment_rolls_instead_of_overflowing_one_file() {
  let rig = Rig::with(Limits::default());
  rig.install();
  let body = "x".repeat(380);
  for index in 0..6000 {
    rig
      .store
      .write(&format!("07-30 12:00:00.000  1  1 I burst: {index} {body}"));
  }
  rig.store.flush();

  let dir = rig.live_launch();
  let names = rig.segment_names(&dir);
  assert_eq!(names.len(), 2);
  for name in names {
    let size = fs::metadata(dir.join(&name)).expect("a segment").len();
    assert!(
      size <= Limits::default().segment_bytes + 4096,
      "{name} is {size} bytes, over the cap",
    );
  }
}

#[test]
fn an_error_segment_is_pinned_and_survives_the_segment_ring() {
  let rig = Rig::with(tiny(256, 1));
  rig.install();
  let dir = rig.live_launch();

  rig.store.record(Level::Error, "t", "the thing that went wrong");
  rig.store.flush();
  assert!(dir.join("0000.keep").exists());

  rig.noise(200);

  assert!(rig.segment_names(&dir).contains(&"0000.log".to_owned()));
  assert!(rig.export(None).contains("the thing that went wrong"));
}

#[test]
fn a_raw_line_at_error_severity_pins_its_segment() {
  let rig = Rig::with(tiny(256, 1));
  rig.install();
  let dir = rig.live_launch();

  rig
    .store
    .write("07-30 12:00:00.000  1  1 E burst: the thing that went wrong");
  rig.store.flush();

  assert!(dir.join("0000.keep").exists());
  assert!(
    rig
      .store
      .archives()
      .iter()
      .any(|archive| archive.current && archive.pinned)
  );
}

#[test]
fn a_raw_line_below_error_does_not_pin() {
  let rig = Rig::with(tiny(4096, 2));
  rig.install();
  let dir = rig.live_launch();

  for level in ['V', 'D', 'I', 'W'] {
    rig
      .store
      .write(&format!("07-30 12:00:00.000  1  1 {level} burst: noise"));
  }
  rig.store.flush();

  assert!(!dir.join("0000.keep").exists());
  assert_eq!(rig.store.archives().first().map(|archive| archive.pinned), Some(false));
}

#[test]
fn non_error_levels_do_not_pin() {
  let rig = Rig::with(tiny(4096, 2));
  rig.install();
  let dir = rig.live_launch();

  for level in [Level::Trace, Level::Debug, Level::Info, Level::Notice, Level::Warn] {
    rig.store.record(level, "t", "noise");
  }
  rig.store.flush();

  assert!(!dir.join("0000.keep").exists());
  assert_eq!(rig.store.archives().first().map(|archive| archive.pinned), Some(false));
}

#[test]
fn the_launch_ring_drops_the_oldest_unpinned_launches() {
  let rig = Rig::with(Limits {
    launches: 3,
    ..Limits::default()
  });
  rig.seed("1700000001000", &[(0, "oldest\n", false)]);
  rig.seed("1700000002000", &[(0, "middle\n", false)]);
  rig.seed("1700000003000", &[(0, "newest\n", false)]);
  rig.install();

  let ids = rig.ids();
  assert_eq!(ids.len(), 3);
  assert!(!ids.contains(&"1700000001000".to_owned()));
  assert!(ids.contains(&"1700000002000".to_owned()));
  assert!(ids.contains(&"1700000003000".to_owned()));
}

#[test]
fn the_launch_ring_ignores_pinned_launches() {
  let rig = Rig::with(Limits {
    launches: 2,
    ..Limits::default()
  });
  rig.seed("1700000001000", &[(0, "pinned oldest\n", true)]);
  rig.seed("1700000002000", &[(0, "plain\n", false)]);
  rig.seed("1700000003000", &[(0, "plain\n", false)]);
  rig.seed("1700000004000", &[(0, "plain\n", false)]);
  rig.install();

  let ids = rig.ids();
  assert!(ids.contains(&"1700000001000".to_owned()));
  assert!(!ids.contains(&"1700000002000".to_owned()));
  assert!(!ids.contains(&"1700000003000".to_owned()));
  assert!(ids.contains(&"1700000004000".to_owned()));
}

#[test]
fn the_pinned_bytes_limit_sheds_the_oldest_pinned_launch() {
  let body = "e".repeat(600);
  let rig = Rig::with(Limits {
    pinned_bytes_limit: 1500,
    ..Limits::default()
  });
  rig.seed("1700000001000", &[(0, &body, true)]);
  rig.seed("1700000002000", &[(0, &body, true)]);
  rig.seed("1700000003000", &[(0, &body, true)]);
  rig.install();

  let ids = rig.ids();
  assert!(!ids.contains(&"1700000001000".to_owned()));
  assert!(ids.contains(&"1700000002000".to_owned()));
  assert!(ids.contains(&"1700000003000".to_owned()));
}

#[test]
fn the_pinned_bytes_limit_keeps_everything_under_the_cap() {
  let body = "e".repeat(100);
  let rig = Rig::with(Limits {
    pinned_bytes_limit: 1500,
    ..Limits::default()
  });
  rig.seed("1700000001000", &[(0, &body, true)]);
  rig.seed("1700000002000", &[(0, &body, true)]);
  rig.install();

  let ids = rig.ids();
  assert!(ids.contains(&"1700000001000".to_owned()));
  assert!(ids.contains(&"1700000002000".to_owned()));
}

#[test]
fn archives_are_newest_first_and_flag_the_live_launch() {
  let rig = Rig::with(Limits::default());
  rig.seed("1700000001000", &[(0, "old\n", false)]);
  rig.seed("1700000002000", &[(0, "err\n", true)]);
  rig.install();
  rig.store.record(Level::Info, "t", "live");
  rig.store.flush();

  let archives = rig.store.archives();
  let stamps: Vec<u64> = archives.iter().map(|archive| archive.started_at_ms).collect();
  let mut descending = stamps.clone();
  descending.sort_by(|left, right| right.cmp(left));
  assert_eq!(stamps, descending);
  assert!(archives[0].current);
  assert_eq!(archives.iter().filter(|archive| archive.current).count(), 1);

  let by_id = |id: &str| archives.iter().find(|archive| archive.id == id).cloned();
  assert_eq!(by_id("1700000002000").map(|archive| archive.pinned), Some(true));
  assert_eq!(by_id("1700000001000").map(|archive| archive.pinned), Some(false));
  assert_eq!(by_id("1700000001000").map(|archive| archive.bytes), Some(4));
}

#[test]
fn retained_bytes_sums_every_launch() {
  let rig = Rig::with(Limits::default());
  rig.seed("1700000001000", &[(0, "12345", false)]);
  rig.seed("1700000002000", &[(0, "123", false), (1, "12", false)]);
  rig.install();

  assert_eq!(rig.store.retained_bytes(), 10);
}

#[test]
fn the_export_header_and_banners_describe_every_launch() {
  let rig = Rig::with(Limits::default());
  rig.seed("1700000001000", &[(0, "from an older run\n", false)]);
  rig.seed("1700000002000", &[(0, "from a run that failed\n", true)]);
  rig.install();
  rig.store.record(Level::Info, "t", "from the live run");
  rig.store.flush();

  let text = rig.export(None);
  assert!(text.starts_with("bridgething log export\n"));
  assert!(text.contains("\nlaunches: 3\n"));
  assert!(!text.contains("dropped lines"));
  assert!(text.contains("[pinned: contains errors]"));
  assert!(text.contains("(current)"));

  let older = text.find("from an older run").expect("the older run");
  let failed = text.find("from a run that failed").expect("the failed run");
  let live = text.find("from the live run").expect("the live run");
  assert!(older < failed);
  assert!(failed < live);
}

#[test]
fn an_export_with_an_id_narrows_to_that_launch() {
  let rig = Rig::with(Limits::default());
  rig.seed("1700000001000", &[(0, "from an older run\n", false)]);
  rig.seed("1700000002000", &[(0, "from a newer run\n", false)]);
  rig.install();

  let text = rig.export(Some("1700000001000"));
  assert!(text.contains("\nlaunches: 1\n"));
  assert!(text.contains("from an older run"));
  assert!(!text.contains("from a newer run"));
}

#[test]
fn an_export_concatenates_segments_in_order() {
  let rig = Rig::with(Limits::default());
  rig.seed(
    "1700000001000",
    &[(0, "alpha\n", false), (1, "beta\n", false), (2, "gamma\n", false)],
  );
  rig.install();

  let text = rig.export(Some("1700000001000"));
  let alpha = text.find("alpha").expect("alpha");
  let beta = text.find("beta").expect("beta");
  let gamma = text.find("gamma").expect("gamma");
  assert!(alpha < beta);
  assert!(beta < gamma);
}

#[test]
fn an_export_terminates_a_segment_that_lacks_a_trailing_newline() {
  let rig = Rig::with(Limits::default());
  rig.seed("1700000001000", &[(0, "unterminated", false), (1, "next\n", false)]);
  rig.install();

  assert!(rig.export(Some("1700000001000")).contains("unterminated\nnext"));
}

#[test]
fn an_export_overwrites_an_existing_bundle() {
  let rig = Rig::with(Limits::default());
  rig.seed("1700000001000", &[(0, "short\n", false)]);
  rig.install();

  let target = rig.path().join("bundle.txt");
  fs::write(&target, "stale".repeat(500)).expect("a stale bundle");
  rig
    .store
    .export_to(&target, Some("1700000001000"))
    .expect("a fresh export");

  let text = fs::read_to_string(&target).expect("the bundle");
  assert!(!text.contains("stale"));
  assert!(text.contains("short"));
}

#[test]
fn read_splits_a_stored_line_back_into_its_fields() {
  let rig = Rig::with(Limits::default());
  rig.install();
  rig.store.record(Level::Warn, "gateway", "the link went away");
  rig.store.flush();

  let id = rig.live_id();
  let lines = rig.store.read(&id, 100);
  let line = lines.iter().find(|line| line.label == "gateway").expect("the record");
  assert_eq!(line.level, Level::Warn);
  assert_eq!(line.message, "the link went away");
  assert!(line.ts_unix_ms > 0);
}

#[test]
fn read_dates_a_line_from_the_launch_it_belongs_to() {
  let rig = Rig::with(Limits::default());
  rig.seed(
    "1700000000000",
    &[(0, "11-14 22:13:20.000  1  1 I daemon: from a past launch\n", false)],
  );
  rig.install();

  let line = rig.store.read("1700000000000", 100).pop().expect("the seeded line");
  let drift = line.ts_unix_ms.abs_diff(1_700_000_000_000);
  assert!(drift < 48 * 60 * 60 * 1000, "dated {} ms from its launch", drift);
}

#[test]
fn read_walks_the_segments_in_order_and_keeps_the_tail() {
  let rig = Rig::with(Limits::default());
  rig.seed(
    "1700000001000",
    &[
      (0, "07-30 12:00:00.000  1  1 I t: alpha\n", false),
      (
        1,
        "07-30 12:00:01.000  1  1 I t: beta\n07-30 12:00:02.000  1  1 I t: gamma\n",
        false,
      ),
    ],
  );
  rig.install();

  let all = rig.store.read("1700000001000", 100);
  let bodies: Vec<&str> = all.iter().map(|line| line.message.as_str()).collect();
  assert_eq!(bodies, vec!["alpha", "beta", "gamma"]);
  assert_eq!(all[1].ts_unix_ms - all[0].ts_unix_ms, 1000);

  let tail = rig.store.read("1700000001000", 2);
  let tail_bodies: Vec<&str> = tail.iter().map(|line| line.message.as_str()).collect();
  assert_eq!(tail_bodies, vec!["beta", "gamma"]);
}

#[test]
fn read_carries_the_last_stamp_onto_a_line_with_no_prefix() {
  let rig = Rig::with(Limits::default());
  rig.seed(
    "1700000001000",
    &[(0, "07-30 12:00:00.000  1  1 I t: stamped\n  at some::frame\n", false)],
  );
  rig.install();

  let lines = rig.store.read("1700000001000", 100);
  assert_eq!(lines[1].message, "  at some::frame");
  assert_eq!(lines[1].ts_unix_ms, lines[0].ts_unix_ms);
}

#[test]
fn read_is_empty_for_an_unknown_archive_or_a_zero_limit() {
  let rig = Rig::with(Limits::default());
  rig.seed("1700000001000", &[(0, "07-30 12:00:00.000  1  1 I t: here\n", false)]);
  rig.install();

  assert!(rig.store.read("1700000009000", 100).is_empty());
  assert!(rig.store.read("1700000001000", 0).is_empty());
}

#[test]
fn a_message_past_the_queue_cap_drops_lines_instead_of_growing() {
  let rig = Rig::with(Limits {
    queue_capacity: 1,
    ..Limits::default()
  });
  rig.install();
  let flood: Vec<String> = (0..5000).map(|index| format!("line {index}")).collect();
  rig.store.record(Level::Info, "t", &flood.join("\n"));
  rig.store.flush();

  assert!(rig.export(None).contains("dropped lines (writer backpressure): 4999"));
}

#[test]
fn delete_drops_a_past_launch_and_leaves_the_rest() {
  let rig = Rig::with(Limits::default());
  rig.seed("1700000001000", &[(0, "gone\n", false)]);
  rig.seed("1700000002000", &[(0, "kept\n", false)]);
  rig.install();
  rig.store.delete("1700000001000");

  let ids = rig.ids();
  assert!(!ids.contains(&"1700000001000".to_owned()));
  assert!(ids.contains(&"1700000002000".to_owned()));
}

#[test]
fn delete_truncates_the_live_launch_and_keeps_recording() {
  let rig = Rig::with(Limits::default());
  rig.install();
  rig.store.record(Level::Info, "t", "before");
  rig.store.flush();

  let id = rig
    .store
    .archives()
    .into_iter()
    .find(|archive| archive.current)
    .expect("a live launch")
    .id;
  rig.store.delete(&id);
  assert_eq!(
    rig
      .store
      .archives()
      .into_iter()
      .find(|archive| archive.id == id)
      .map(|archive| archive.bytes),
    Some(0)
  );

  rig.store.record(Level::Info, "t", "after");
  rig.store.flush();

  let text = rig.export(None);
  assert!(!text.contains("before"));
  assert!(text.contains("after"));
}

#[test]
fn delete_ignores_ids_that_are_not_launch_directories() {
  let rig = Rig::with(Limits::default());
  rig.seed("1700000001000", &[(0, "kept\n", false)]);
  rig.install();

  rig.store.delete("../1700000001000");
  rig.store.delete("not-a-launch");
  rig.store.delete("");

  assert!(rig.ids().contains(&"1700000001000".to_owned()));
}

#[test]
fn clear_removes_pinned_launches_and_truncates_the_live_one() {
  let rig = Rig::with(Limits::default());
  rig.seed("1700000001000", &[(0, "plain\n", false)]);
  rig.seed("1700000002000", &[(0, "pinned\n", true)]);
  rig.install();
  rig.store.record(Level::Error, "t", "live error");
  rig.store.flush();

  rig.store.clear();

  let archives = rig.store.archives();
  assert_eq!(archives.len(), 1);
  assert!(archives[0].current);
  assert_eq!(archives[0].bytes, 0);
  assert!(!archives[0].pinned);
  assert_eq!(rig.store.retained_bytes(), 0);
}

#[test]
fn a_cleared_live_launch_still_rotates_afterwards() {
  let rig = Rig::with(tiny(256, 2));
  rig.install();
  let dir = rig.live_launch();

  rig.store.record(Level::Error, "t", "pin me");
  rig.store.flush();
  rig.store.clear();

  rig.noise(200);

  assert_eq!(rig.segment_names(&dir).len(), 2);
  let text = rig.export(None);
  assert!(text.contains("line 199"));
  assert!(!text.contains("pin me"));
}

#[test]
fn install_is_idempotent() {
  let rig = Rig::with(Limits::default());
  rig.install();
  let first = rig.store.archives().first().expect("a live launch").id.clone();
  rig.install();
  assert_eq!(rig.store.archives().len(), 1);
  assert_eq!(
    rig.store.archives().first().map(|archive| archive.id.clone()),
    Some(first)
  );
}
