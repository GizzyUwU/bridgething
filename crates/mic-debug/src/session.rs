use std::{
  fs::{self, File, OpenOptions},
  io::{self, Write},
  os::fd::AsRawFd,
  path::{Path, PathBuf},
  time::{SystemTime, UNIX_EPOCH},
};

use bridgething_dsp::geometry::CHANNELS;
use serde::Serialize;
use serde_json::json;

use crate::status::SAMPLE_RATE_HZ;

pub const SEGMENT_SECS: u64 = 60;
pub const RAW_BYTES_PER_FRAME: u64 = CHANNELS as u64 * 4;
pub const BEAM_BYTES_PER_SAMPLE: u64 = 2;
pub const FRAMES_PER_SEGMENT: u64 = SAMPLE_RATE_HZ as u64 * SEGMENT_SECS;
pub const BYTES_PER_SEC: u64 = SAMPLE_RATE_HZ as u64 * (RAW_BYTES_PER_FRAME + BEAM_BYTES_PER_SAMPLE);

const SESSION_ROOT: &str = "mic-debug";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
  pub session: String,
  pub sample_rate_hz: u32,
  pub raw_channels: usize,
  pub raw_format: &'static str,
  pub beam_format: &'static str,
  pub frames_per_segment: u64,
  pub wakeword_model: Option<String>,
  pub wakeword_threshold: f32,
  pub steering_deg: f64,
  pub adaptation: bool,
  pub started_wall_unix: Option<u64>,
  pub kernel: String,
  pub image: Option<String>,
  pub notes: String,
}

struct Track {
  dir: PathBuf,
  kind: &'static str,
  bytes_per_sample: u64,
  file: File,
  segment: u64,
  written: u64,
  samples: u64,
}

impl Track {
  fn open(dir: &Path, kind: &'static str, bytes_per_sample: u64) -> io::Result<Self> {
    fs::create_dir_all(dir.join(kind))?;
    Ok(Self {
      file: open_segment(dir, kind, 0, FRAMES_PER_SEGMENT * bytes_per_sample)?,
      dir: dir.to_path_buf(),
      kind,
      bytes_per_sample,
      segment: 0,
      written: 0,
      samples: 0,
    })
  }

  fn segment_bytes(&self) -> u64 {
    FRAMES_PER_SEGMENT * self.bytes_per_sample
  }

  fn write(&mut self, mut data: &[u8]) -> io::Result<()> {
    while !data.is_empty() {
      let room = (self.segment_bytes() - self.written) as usize;
      let take = room.min(data.len());
      self.file.write_all(&data[..take])?;
      self.written += take as u64;
      self.samples += take as u64 / self.bytes_per_sample;
      data = &data[take..];
      if self.written == self.segment_bytes() {
        self.rotate()?;
      }
    }
    Ok(())
  }

  fn rotate(&mut self) -> io::Result<()> {
    self.file.sync_data()?;
    self.segment += 1;
    self.written = 0;
    self.file = open_segment(&self.dir, self.kind, self.segment, self.segment_bytes())?;
    sync_dir(&self.dir.join(self.kind))
  }
}

pub struct Session {
  pub name: String,
  dir: PathBuf,
  journal: File,
  raw: Track,
  beam: Track,
}

impl Session {
  pub fn open(mount: &Path, mut meta: SessionMeta) -> io::Result<Self> {
    let root = mount.join(SESSION_ROOT);
    fs::create_dir_all(&root)?;
    let name = next_session_name(&root)?;
    let dir = root.join(&name);
    fs::create_dir_all(&dir)?;

    meta.session = name.clone();
    let mut meta_file = File::create(dir.join("meta.json"))?;
    meta_file.write_all(serde_json::to_string_pretty(&meta)?.as_bytes())?;
    meta_file.write_all(b"\n")?;
    meta_file.sync_all()?;

    let journal = OpenOptions::new()
      .create(true)
      .append(true)
      .open(dir.join("journal.jsonl"))?;
    let mut session = Self {
      name,
      raw: Track::open(&dir, "raw", RAW_BYTES_PER_FRAME)?,
      beam: Track::open(&dir, "beam", BEAM_BYTES_PER_SAMPLE)?,
      dir,
      journal,
    };
    sync_dir(&session.dir)?;
    sync_dir(&root)?;
    session.record("open", json!({}))?;
    session.sync()?;
    Ok(session)
  }

  #[cfg(test)]
  pub fn raw_frames(&self) -> u64 {
    self.raw.samples
  }

  pub fn recorded_secs(&self) -> u64 {
    self.raw.samples / SAMPLE_RATE_HZ as u64
  }

  pub fn write_audio(&mut self, raw: &[u8], beam: &[u8]) -> io::Result<()> {
    self.raw.write(raw)?;
    self.beam.write(beam)
  }

  pub fn record(&mut self, kind: &str, body: serde_json::Value) -> io::Result<()> {
    let mut line = json!({
      "t": kind,
      "rawFrame": self.raw.samples,
      "beamSample": self.beam.samples,
      "monoNs": monotonic_ns(),
      "wallUnix": wall_unix(),
    });
    if let (Some(line), Some(body)) = (line.as_object_mut(), body.as_object()) {
      for (key, value) in body {
        line.entry(key.clone()).or_insert(value.clone());
      }
    }
    let mut text = serde_json::to_string(&line)?;
    text.push('\n');
    self.journal.write_all(text.as_bytes())
  }

  pub fn sync(&mut self) -> io::Result<()> {
    self.record("progress", json!({}))?;
    self.raw.file.sync_data()?;
    self.beam.file.sync_data()?;
    self.journal.sync_data()
  }

  pub fn close(mut self, why: &str) -> io::Result<()> {
    self.record("close", json!({ "why": why }))?;
    self.raw.file.sync_all()?;
    self.beam.file.sync_all()?;
    self.journal.sync_all()?;
    sync_dir(&self.dir)
  }
}

fn open_segment(dir: &Path, kind: &str, index: u64, size: u64) -> io::Result<File> {
  let path = dir.join(kind).join(format!("{index:06}.pcm"));
  let file = OpenOptions::new().create(true).write(true).truncate(true).open(path)?;
  let rc = unsafe { libc::posix_fallocate(file.as_raw_fd(), 0, size as libc::off_t) };
  if rc != 0 {
    tracing::warn!(
      "could not preallocate a {kind} segment: {}",
      io::Error::from_raw_os_error(rc)
    );
  }
  Ok(file)
}

fn sync_dir(dir: &Path) -> io::Result<()> {
  File::open(dir)?.sync_all()
}

fn next_session_name(root: &Path) -> io::Result<String> {
  let mut highest = 0u32;
  for entry in fs::read_dir(root)? {
    let name = entry?.file_name().to_string_lossy().into_owned();
    if let Some(number) = name.strip_prefix("session-").and_then(|n| n.parse::<u32>().ok()) {
      highest = highest.max(number);
    }
  }
  Ok(format!("session-{:04}", highest + 1))
}

fn monotonic_ns() -> u64 {
  let mut ts: libc::timespec = unsafe { std::mem::zeroed() };
  unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
  ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
}

pub fn wall_unix() -> Option<u64> {
  let secs = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
  (secs > 1_577_836_800).then_some(secs)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn meta() -> SessionMeta {
    SessionMeta {
      session: String::new(),
      sample_rate_hz: SAMPLE_RATE_HZ,
      raw_channels: CHANNELS,
      raw_format: "s32le",
      beam_format: "s16le",
      frames_per_segment: FRAMES_PER_SEGMENT,
      wakeword_model: None,
      wakeword_threshold: 0.35,
      steering_deg: 35.0,
      adaptation: true,
      started_wall_unix: None,
      kernel: "test".into(),
      image: None,
      notes: String::new(),
    }
  }

  fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mic-debug-test-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
  }

  #[test]
  fn a_session_is_readable_before_any_audio_arrives() {
    let mount = scratch("meta-first");
    let session = Session::open(&mount, meta()).expect("open");
    let written = fs::read_to_string(mount.join(SESSION_ROOT).join(&session.name).join("meta.json")).expect("meta");
    assert!(written.contains("\"sampleRateHz\": 16000"));
    assert!(written.contains("\"rawChannels\": 4"));
  }

  #[test]
  fn sessions_number_upward_and_never_reuse_a_directory() {
    let mount = scratch("numbering");
    let first = Session::open(&mount, meta()).expect("first");
    assert_eq!(first.name, "session-0001");
    first.close("test").expect("close");
    let second = Session::open(&mount, meta()).expect("second");
    assert_eq!(second.name, "session-0002");
  }

  #[test]
  fn audio_rolls_into_a_new_segment_on_the_frame_boundary() {
    let mount = scratch("rotation");
    let mut session = Session::open(&mount, meta()).expect("open");
    let raw = vec![0u8; (FRAMES_PER_SEGMENT * RAW_BYTES_PER_FRAME) as usize];
    let beam = vec![0u8; (FRAMES_PER_SEGMENT * BEAM_BYTES_PER_SAMPLE) as usize];
    session.write_audio(&raw, &beam).expect("write");
    session
      .write_audio(&raw[..RAW_BYTES_PER_FRAME as usize], &beam[..2])
      .expect("write");
    let dir = mount.join(SESSION_ROOT).join(&session.name);
    assert!(dir.join("raw/000000.pcm").exists());
    assert!(
      dir.join("raw/000001.pcm").exists(),
      "the boundary must open a new segment"
    );
    assert_eq!(session.raw_frames(), FRAMES_PER_SEGMENT + 1);
  }

  #[test]
  fn a_write_straddling_the_boundary_is_split_so_segment_n_starts_at_sample_n_times_the_length() {
    let mount = scratch("exact-split");
    let mut session = Session::open(&mount, meta()).expect("open");
    let frames = FRAMES_PER_SEGMENT + FRAMES_PER_SEGMENT / 4;
    session
      .write_audio(
        &vec![0u8; (frames * RAW_BYTES_PER_FRAME) as usize],
        &vec![0u8; (frames * BEAM_BYTES_PER_SAMPLE) as usize],
      )
      .expect("write");

    let dir = mount.join(SESSION_ROOT).join(&session.name);
    for (kind, unit) in [("raw", RAW_BYTES_PER_FRAME), ("beam", BEAM_BYTES_PER_SAMPLE)] {
      assert_eq!(
        fs::metadata(dir.join(kind).join("000000.pcm")).expect("stat").len(),
        FRAMES_PER_SEGMENT * unit,
        "{kind} segment 0 must hold exactly one segment of samples and no more"
      );
    }
    assert_eq!(session.raw_frames(), frames);
  }

  #[test]
  fn a_segment_is_full_length_on_disk_before_it_is_written() {
    let mount = scratch("prealloc");
    let session = Session::open(&mount, meta()).expect("open");
    let len = fs::metadata(mount.join(SESSION_ROOT).join(&session.name).join("raw/000000.pcm"))
      .expect("stat")
      .len();
    assert_eq!(
      len,
      FRAMES_PER_SEGMENT * RAW_BYTES_PER_FRAME,
      "a cut mid-session must leave a full-length file, not one whose size never reached the disk"
    );
  }

  #[test]
  fn every_journal_record_carries_the_sample_offset_it_happened_at() {
    let mount = scratch("journal-offsets");
    let mut session = Session::open(&mount, meta()).expect("open");
    let raw = vec![0u8; (1000 * RAW_BYTES_PER_FRAME) as usize];
    session.write_audio(&raw, &[0u8; 2000]).expect("write");
    session.record("mark", json!({ "kind": "utterance" })).expect("record");
    session.sync().expect("sync");

    let journal = fs::read_to_string(mount.join(SESSION_ROOT).join(&session.name).join("journal.jsonl")).expect("read");
    let mark: serde_json::Value = journal
      .lines()
      .map(|line| serde_json::from_str(line).expect("json line"))
      .find(|value: &serde_json::Value| value["t"] == "mark")
      .expect("mark record");
    assert_eq!(mark["rawFrame"], 1000);
    assert_eq!(mark["beamSample"], 1000);
  }

  #[test]
  fn the_progress_record_is_what_bounds_the_real_audio_in_a_preallocated_segment() {
    let mount = scratch("progress");
    let mut session = Session::open(&mount, meta()).expect("open");
    session
      .write_audio(&vec![0u8; (4 * RAW_BYTES_PER_FRAME) as usize], &[0u8; 16])
      .expect("write");
    session.sync().expect("sync");

    let journal = fs::read_to_string(mount.join(SESSION_ROOT).join(&session.name).join("journal.jsonl")).expect("read");
    let last: serde_json::Value = journal
      .lines()
      .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
      .filter(|value| value["t"] == "progress")
      .next_back()
      .expect("progress record");
    assert_eq!(last["rawFrame"], 4, "a zero tail in the preallocated file is not audio");
    assert_eq!(last["beamSample"], 8);
  }

  #[test]
  fn a_record_body_cannot_overwrite_the_offsets_it_is_stamped_with() {
    let mount = scratch("head-wins");
    let mut session = Session::open(&mount, meta()).expect("open");
    session
      .write_audio(&vec![0u8; (7 * RAW_BYTES_PER_FRAME) as usize], &[0u8; 14])
      .expect("write");
    session
      .record("mark", json!({ "rawFrame": 999_999, "kind": "utterance" }))
      .expect("record");
    session.sync().expect("sync");

    let journal = fs::read_to_string(mount.join(SESSION_ROOT).join(&session.name).join("journal.jsonl")).expect("read");
    let mark: serde_json::Value = journal
      .lines()
      .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
      .find(|value: &serde_json::Value| value["t"] == "mark")
      .expect("mark record");
    assert_eq!(mark["rawFrame"], 7);
    assert_eq!(mark["kind"], "utterance");
  }
}
